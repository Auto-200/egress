use anyhow::{anyhow, bail, Context, Result};
use arc_swap::ArcSwap;
use bytes::Bytes;
use clap::Parser;
use egress_proto::{
    HealthConfig, NodeEndpoint, RouterConfig, RouterControlPlaneConfig, RouterLineConfig,
    RouterTlsConfig,
};
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use prometheus::{
    Encoder, HistogramOpts, HistogramVec, IntCounterVec, IntGaugeVec, Opts, Registry, TextEncoder,
};
use reqwest::Client;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use rustls::RootCertStore;
use std::convert::Infallible;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{copy_bidirectional, AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinSet;
use tokio::time::timeout;
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;
use tracing::{debug, info, warn};

#[derive(Debug, Parser)]
#[command(name = "su8-egress-router")]
struct Args {
    #[arg(long, default_value = "config.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(tracing::Level::INFO.into())
        .from_env_lossy();
    tracing_subscriber::fmt().with_env_filter(filter).init();
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let args = Args::parse();
    let raw = std::fs::read_to_string(&args.config)
        .with_context(|| format!("read config {}", args.config.display()))?;
    let cfg: RouterConfig = toml::from_str(&raw).context("parse router config")?;
    let connector = build_connector(&cfg.tls)?;
    let http = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .context("build control plane HTTP client")?;
    let line = cfg.line.clone();
    let control = cfg.control_plane.clone();
    let initial_pool = if let Some(cp) = &control {
        match fetch_line_config(&http, cp, &line, None).await {
            Ok(ConfigFetch::Updated(config)) => Pool::from_config(config, None),
            Ok(ConfigFetch::Gone) => {
                warn!(%line, "line missing or disabled at startup; starting with an empty pool");
                Pool::empty("gone".to_string())
            }
            Ok(ConfigFetch::NotModified) => Pool::empty("unknown".to_string()),
            Err(err) => {
                warn!(%line, error = %err, "control plane unavailable at startup; starting with an empty pool");
                Pool::empty("unavailable".to_string())
            }
        }
    } else {
        Pool::new(
            cfg.nodes.clone(),
            "file".to_string(),
            "least-connections".to_string(),
        )
    };
    let pool = Arc::new(ArcSwap::from_pointee(initial_pool));
    let metrics = Metrics::new()?;
    let state = Arc::new(State {
        cfg: Arc::new(cfg),
        pool: pool.clone(),
        connector: connector.clone(),
        metrics,
    });

    if let Some(cp) = control.clone() {
        tokio::spawn(config_poll_loop(
            cp,
            line.clone(),
            pool.clone(),
            http.clone(),
        ));
    }
    tokio::spawn(health_loop(
        pool.clone(),
        connector.clone(),
        state.cfg.health.clone(),
        control,
        http.clone(),
    ));

    let listener = TcpListener::bind(state.cfg.listen).await?;
    info!(
        listen = %state.cfg.listen,
        line = %state.cfg.line,
        nodes = state.pool.load().nodes.len(),
        "egress router listening"
    );
    loop {
        let (tcp, peer) = listener.accept().await?;
        let state = state.clone();
        tokio::spawn(async move {
            let service = service_fn(move |req| handle(req, state.clone()));
            if let Err(err) = http1::Builder::new()
                .serve_connection(TokioIo::new(tcp), service)
                .with_upgrades()
                .await
            {
                warn!(%peer, error = %err, "connection error");
            }
        });
    }
}

struct State {
    cfg: Arc<RouterConfig>,
    pool: Arc<ArcSwap<Pool>>,
    connector: TlsConnector,
    metrics: Metrics,
}

struct NodeState {
    cfg: NodeEndpoint,
    healthy: AtomicBool,
    failures: AtomicU32,
    successes: AtomicU32,
    active: AtomicU64,
}

struct Pool {
    nodes: Vec<Arc<NodeState>>,
    rr: AtomicUsize,
    version: String,
    policy: String,
}

struct ActiveGuard(Arc<NodeState>);

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::Relaxed);
    }
}

impl Pool {
    fn new(nodes: Vec<NodeEndpoint>, version: String, policy: String) -> Self {
        Self {
            nodes: nodes
                .into_iter()
                .map(|cfg| {
                    Arc::new(NodeState {
                        cfg,
                        healthy: AtomicBool::new(false),
                        failures: AtomicU32::new(0),
                        successes: AtomicU32::new(0),
                        active: AtomicU64::new(0),
                    })
                })
                .collect(),
            rr: AtomicUsize::new(0),
            version,
            policy,
        }
    }

    fn from_config(config: RouterLineConfig, old: Option<&Pool>) -> Self {
        let RouterLineConfig {
            version,
            line: _,
            policy,
            nodes,
        } = config;
        let old_health: std::collections::HashMap<&str, &Arc<NodeState>> = old
            .map(|pool| {
                pool.nodes
                    .iter()
                    .map(|node| (node.cfg.name.as_str(), node))
                    .collect()
            })
            .unwrap_or_default();
        let nodes = nodes
            .into_iter()
            .map(|cfg| {
                let healthy = old_health
                    .get(cfg.name.as_str())
                    .map(|node| node.healthy.load(Ordering::Relaxed))
                    .unwrap_or(false);
                Arc::new(NodeState {
                    cfg,
                    healthy: AtomicBool::new(healthy),
                    failures: AtomicU32::new(0),
                    successes: AtomicU32::new(0),
                    active: AtomicU64::new(0),
                })
            })
            .collect();
        Self {
            nodes,
            rr: AtomicUsize::new(0),
            version,
            policy,
        }
    }

    fn empty(version: String) -> Self {
        Self {
            nodes: Vec::new(),
            rr: AtomicUsize::new(0),
            version,
            policy: "least-connections".to_string(),
        }
    }

    fn pick(&self) -> Option<(Arc<NodeState>, ActiveGuard)> {
        let healthy: Vec<&Arc<NodeState>> = self
            .nodes
            .iter()
            .filter(|node| node.healthy.load(Ordering::Relaxed))
            .collect();
        if healthy.is_empty() {
            return None;
        }
        let start = self.rr.fetch_add(1, Ordering::Relaxed) % healthy.len();
        if self.policy == "round-robin" {
            let chosen = Arc::clone(healthy[start]);
            chosen.active.fetch_add(1, Ordering::Relaxed);
            return Some((Arc::clone(&chosen), ActiveGuard(chosen)));
        }
        let mut best = healthy[start];
        for node in &healthy {
            if node.active.load(Ordering::Relaxed) < best.active.load(Ordering::Relaxed) {
                best = node;
            }
        }
        let chosen = Arc::clone(best);
        chosen.active.fetch_add(1, Ordering::Relaxed);
        Some((Arc::clone(&chosen), ActiveGuard(chosen)))
    }

    fn any_healthy(&self) -> bool {
        self.nodes
            .iter()
            .any(|node| node.healthy.load(Ordering::Relaxed))
    }
}

async fn health_loop(
    pool: Arc<ArcSwap<Pool>>,
    connector: TlsConnector,
    health: HealthConfig,
    control: Option<RouterControlPlaneConfig>,
    http: Client,
) {
    let interval = Duration::from_secs(health.interval_secs.max(1));
    loop {
        let snapshot = pool.load_full();
        let mut set = JoinSet::new();
        for node in &snapshot.nodes {
            let node = Arc::clone(node);
            let connector = connector.clone();
            set.spawn(async move {
                let started = Instant::now();
                let ok = check_node(&node, &connector).await;
                let latency_ms = started.elapsed().as_millis() as i64;
                (node, ok, latency_ms)
            });
        }
        while let Some(result) = set.join_next().await {
            if let Ok((node, ok, latency_ms)) = result {
                record_health(&node, ok, &health);
                if let Some(control) = &control {
                    report_health(&http, control, &node.cfg.name, ok, latency_ms).await;
                }
            }
        }
        tokio::time::sleep(interval).await;
    }
}

async fn config_poll_loop(
    control: RouterControlPlaneConfig,
    line: String,
    pool: Arc<ArcSwap<Pool>>,
    http: Client,
) {
    let interval = Duration::from_secs(control.poll_interval_secs.max(1));
    loop {
        let etag = format!("\"{}\"", pool.load().version);
        match fetch_line_config(&http, &control, &line, Some(&etag)).await {
            Ok(ConfigFetch::Updated(config)) => {
                let version = config.version.clone();
                let old = pool.load_full();
                let next = Pool::from_config(config, Some(&old));
                let nodes = next.nodes.len();
                pool.store(Arc::new(next));
                info!(%line, version = %version, nodes, "router config updated");
            }
            Ok(ConfigFetch::Gone) => {
                let old = pool.load_full();
                if !old.nodes.is_empty() {
                    pool.store(Arc::new(Pool::empty("gone".to_string())));
                    warn!(%line, "control plane reports line gone; clearing node pool");
                }
            }
            Ok(ConfigFetch::NotModified) => {}
            Err(err) => warn!(%line, error = %err, "router config poll failed"),
        }
        tokio::time::sleep(interval).await;
    }
}

enum ConfigFetch {
    NotModified,
    Updated(RouterLineConfig),
    Gone,
}

async fn fetch_line_config(
    http: &Client,
    control: &RouterControlPlaneConfig,
    line: &str,
    etag: Option<&str>,
) -> Result<ConfigFetch> {
    let url = format!(
        "{}/internal/v1/config/{}",
        control.url.trim_end_matches('/'),
        urlencoding::encode(line)
    );
    let mut request = http.get(&url).bearer_auth(&control.token);
    if let Some(etag) = etag {
        request = request.header(reqwest::header::IF_NONE_MATCH, etag);
    }
    let response = request.send().await.context("control plane request")?;
    match response.status() {
        reqwest::StatusCode::NOT_MODIFIED => Ok(ConfigFetch::NotModified),
        reqwest::StatusCode::NOT_FOUND => Ok(ConfigFetch::Gone),
        status if status.is_success() => {
            let config = response
                .json::<RouterLineConfig>()
                .await
                .context("parse line config")?;
            Ok(ConfigFetch::Updated(config))
        }
        status => bail!("control plane returned {status}"),
    }
}

async fn report_health(
    http: &Client,
    control: &RouterControlPlaneConfig,
    node: &str,
    healthy: bool,
    latency_ms: i64,
) {
    let url = format!(
        "{}/internal/v1/health/{}",
        control.url.trim_end_matches('/'),
        urlencoding::encode(node)
    );
    let body = serde_json::json!({
        "healthy": healthy,
        "latency_ms": latency_ms,
        "detail": if healthy { "ok" } else { "healthz failed" },
    });
    match http
        .post(&url)
        .bearer_auth(&control.token)
        .json(&body)
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => {}
        Ok(response) => {
            debug!(%node, status = %response.status(), "health report rejected");
        }
        Err(err) => debug!(%node, error = %err, "health report failed"),
    }
}

fn record_health(node: &NodeState, ok: bool, health: &HealthConfig) {
    if ok {
        node.failures.store(0, Ordering::Relaxed);
        let successes = node.successes.fetch_add(1, Ordering::Relaxed) + 1;
        if successes >= health.success_threshold.max(1)
            && !node.healthy.swap(true, Ordering::Relaxed)
        {
            info!(node = %node.cfg.name, "egress node healthy");
        }
    } else {
        node.successes.store(0, Ordering::Relaxed);
        let failures = node.failures.fetch_add(1, Ordering::Relaxed) + 1;
        if failures >= health.failure_threshold.max(1)
            && node.healthy.swap(false, Ordering::Relaxed)
        {
            warn!(node = %node.cfg.name, "egress node unhealthy");
        }
    }
}

async fn check_node(node: &NodeState, connector: &TlsConnector) -> bool {
    let result = async {
        let tcp = timeout(
            Duration::from_secs(5),
            TcpStream::connect(&node.cfg.endpoint),
        )
        .await??;
        let server_name = ServerName::try_from(node.cfg.server_name.clone())?;
        let mut tls = connector.connect(server_name, tcp).await?;
        tls.write_all(
            format!(
                "GET /healthz HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
                node.cfg.server_name
            )
            .as_bytes(),
        )
        .await?;
        let mut buf = [0u8; 64];
        let n = tls.read(&mut buf).await?;
        let head = String::from_utf8_lossy(&buf[..n]);
        Ok::<bool, anyhow::Error>(
            head.starts_with("HTTP/1.1 200") || head.starts_with("HTTP/1.0 200"),
        )
    };
    matches!(timeout(Duration::from_secs(5), result).await, Ok(Ok(true)))
}

async fn handle(
    req: Request<Incoming>,
    state: Arc<State>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    if req.method() == Method::CONNECT {
        return Ok(handle_connect(req, state).await);
    }
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/livez") => Ok(text(StatusCode::OK, "ok\n")),
        (&Method::GET, "/healthz") => {
            if state.pool.load().any_healthy() {
                Ok(text(StatusCode::OK, "ok\n"))
            } else {
                Ok(text(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "no healthy egress node\n",
                ))
            }
        }
        (&Method::GET, "/metrics") => Ok(metrics_response(&state)),
        _ => Ok(text(StatusCode::NOT_FOUND, "not found\n")),
    }
}

async fn handle_connect(req: Request<Incoming>, state: Arc<State>) -> Response<Full<Bytes>> {
    let Some(authority) = req.uri().authority().map(|value| value.to_string()) else {
        return text(StatusCode::BAD_REQUEST, "missing authority\n");
    };
    let pool = state.pool.load_full();
    let Some((node, guard)) = pool.pick() else {
        state
            .metrics
            .connects
            .with_label_values(&[state.cfg.line.as_str(), "-", "no_node"])
            .inc();
        return text(StatusCode::SERVICE_UNAVAILABLE, "no healthy egress node\n");
    };

    let line = state.cfg.line.clone();
    let node_name = node.cfg.name.clone();
    let started = Instant::now();
    let connect_timeout = Duration::from_secs(state.cfg.connect_timeout_secs.max(1));
    let upstream = match connect_node(&node, &authority, &state.connector, connect_timeout).await {
        Ok(stream) => stream,
        Err(err) => {
            let status = err.downcast_ref::<NodeStatusError>().map(|value| value.0);
            let result = if status == Some(403) {
                "denied"
            } else {
                "error"
            };
            state
                .metrics
                .connects
                .with_label_values(&[&line, &node_name, result])
                .inc();
            warn!(%authority, node = %node_name, status = ?status, error = %err, "egress node connect failed");
            if status == Some(403) {
                return text(StatusCode::FORBIDDEN, "target not allowed\n");
            }
            return text(StatusCode::BAD_GATEWAY, "egress node connect failed\n");
        }
    };
    state
        .metrics
        .connect_seconds
        .with_label_values(&[&line])
        .observe(started.elapsed().as_secs_f64());

    let on_upgrade = hyper::upgrade::on(req);
    let connects = state.metrics.connects.clone();
    let bytes = state.metrics.bytes.clone();
    tokio::spawn(async move {
        let _guard = guard;
        match on_upgrade.await {
            Ok(upgraded) => {
                connects.with_label_values(&[&line, &node_name, "ok"]).inc();
                let mut client = TokioIo::new(upgraded);
                let mut upstream = upstream;
                match copy_bidirectional(&mut client, &mut upstream).await {
                    Ok((up, down)) => {
                        bytes
                            .with_label_values(&[&line, &node_name, "up"])
                            .inc_by(up);
                        bytes
                            .with_label_values(&[&line, &node_name, "down"])
                            .inc_by(down);
                    }
                    Err(err) => {
                        warn!(%authority, node = %node_name, error = %err, "tunnel copy failed")
                    }
                }
            }
            Err(err) => {
                connects
                    .with_label_values(&[&line, &node_name, "upgrade_error"])
                    .inc();
                warn!(%authority, node = %node_name, error = %err, "upgrade failed");
            }
        }
    });

    Response::new(Full::new(Bytes::new()))
}

#[derive(Debug, thiserror::Error)]
#[error("node returned {0}")]
struct NodeStatusError(u16);

async fn connect_node(
    node: &NodeState,
    authority: &str,
    connector: &TlsConnector,
    connect_timeout: Duration,
) -> Result<TlsStream<TcpStream>> {
    let tcp = timeout(connect_timeout, TcpStream::connect(&node.cfg.endpoint))
        .await
        .context("node connect timeout")??;
    let server_name = ServerName::try_from(node.cfg.server_name.clone())?;
    let mut tls = timeout(connect_timeout, connector.connect(server_name, tcp))
        .await
        .context("node TLS timeout")??;
    let head = format!("CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\n\r\n");
    tls.write_all(head.as_bytes()).await?;
    let response = timeout(connect_timeout, read_head(&mut tls))
        .await
        .context("node CONNECT timeout")??;
    let status = parse_status(&response).ok_or_else(|| anyhow!("invalid node response"))?;
    if status != 200 {
        return Err(NodeStatusError(status).into());
    }
    Ok(tls)
}

async fn read_head<S: AsyncRead + Unpin>(stream: &mut S) -> Result<Vec<u8>> {
    let mut head = Vec::with_capacity(256);
    let mut byte = [0u8; 1];
    loop {
        let n = stream.read(&mut byte).await?;
        if n == 0 {
            bail!("eof before response head");
        }
        head.push(byte[0]);
        if head.len() > 8192 {
            bail!("response head too large");
        }
        if head.ends_with(b"\r\n\r\n") {
            return Ok(head);
        }
    }
}

fn parse_status(head: &[u8]) -> Option<u16> {
    let text = std::str::from_utf8(head).ok()?;
    let line = text.lines().next()?;
    let mut parts = line.split_whitespace();
    let version = parts.next()?;
    if !version.starts_with("HTTP/") {
        return None;
    }
    parts.next()?.parse().ok()
}

fn text(status: StatusCode, body: &'static str) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header("content-type", "text/plain; charset=utf-8")
        .body(Full::new(Bytes::from_static(body.as_bytes())))
        .expect("static response")
}

fn metrics_response(state: &State) -> Response<Full<Bytes>> {
    let pool = state.pool.load_full();
    let line = state.cfg.line.as_str();
    for node in &pool.nodes {
        state
            .metrics
            .node_healthy
            .with_label_values(&[line, node.cfg.name.as_str()])
            .set(if node.healthy.load(Ordering::Relaxed) {
                1
            } else {
                0
            });
        state
            .metrics
            .node_active
            .with_label_values(&[line, node.cfg.name.as_str()])
            .set(node.active.load(Ordering::Relaxed) as i64);
    }
    let mut buffer = Vec::new();
    let encoder = TextEncoder::new();
    if let Err(err) = encoder.encode(&state.metrics.registry.gather(), &mut buffer) {
        warn!(error = %err, "metrics encode failed");
        return text(StatusCode::INTERNAL_SERVER_ERROR, "metrics encode failed\n");
    }
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", encoder.format_type())
        .body(Full::new(Bytes::from(buffer)))
        .expect("metrics response")
}

struct Metrics {
    registry: Registry,
    node_healthy: IntGaugeVec,
    node_active: IntGaugeVec,
    connects: IntCounterVec,
    connect_seconds: HistogramVec,
    bytes: IntCounterVec,
}

impl Metrics {
    fn new() -> Result<Self> {
        let registry = Registry::new();
        let node_healthy = IntGaugeVec::new(
            Opts::new(
                "su8_egress_router_node_healthy",
                "Egress node health (1 healthy, 0 unhealthy)",
            ),
            &["line", "node"],
        )?;
        let node_active = IntGaugeVec::new(
            Opts::new(
                "su8_egress_router_node_active_connections",
                "Active tunnels per egress node",
            ),
            &["line", "node"],
        )?;
        let connects = IntCounterVec::new(
            Opts::new(
                "su8_egress_router_connect_total",
                "CONNECT attempts through the router",
            ),
            &["line", "node", "result"],
        )?;
        let connect_seconds = HistogramVec::new(
            HistogramOpts::new(
                "su8_egress_router_connect_seconds",
                "Time to establish a node tunnel",
            ),
            &["line"],
        )?;
        let bytes = IntCounterVec::new(
            Opts::new(
                "su8_egress_router_bytes_total",
                "Bytes forwarded through the router",
            ),
            &["line", "node", "direction"],
        )?;
        registry.register(Box::new(node_healthy.clone()))?;
        registry.register(Box::new(node_active.clone()))?;
        registry.register(Box::new(connects.clone()))?;
        registry.register(Box::new(connect_seconds.clone()))?;
        registry.register(Box::new(bytes.clone()))?;
        Ok(Self {
            registry,
            node_healthy,
            node_active,
            connects,
            connect_seconds,
            bytes,
        })
    }
}

fn build_connector(tls: &RouterTlsConfig) -> Result<TlsConnector> {
    let mut roots = RootCertStore::empty();
    for cert in load_certs(&tls.ca)? {
        roots.add(cert).context("add CA")?;
    }
    let mut config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_client_auth_cert(
            load_certs(&tls.client_cert)?,
            load_private_key(&tls.client_key)?,
        )
        .context("build client TLS config")?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(TlsConnector::from(Arc::new(config)))
}

fn load_certs(path: &Path) -> Result<Vec<CertificateDer<'static>>> {
    let mut reader =
        BufReader::new(File::open(path).with_context(|| format!("open {}", path.display()))?);
    let certs = rustls_pemfile::certs(&mut reader)
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("read certs {}", path.display()))?;
    if certs.is_empty() {
        return Err(anyhow!("no certificates in {}", path.display()));
    }
    Ok(certs)
}

fn load_private_key(path: &Path) -> Result<PrivateKeyDer<'static>> {
    let mut reader =
        BufReader::new(File::open(path).with_context(|| format!("open {}", path.display()))?);
    rustls_pemfile::private_key(&mut reader)
        .with_context(|| format!("read private key {}", path.display()))?
        .ok_or_else(|| anyhow!("no private key in {}", path.display()))
}
