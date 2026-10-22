use anyhow::{anyhow, bail, Context, Result};
use arc_swap::ArcSwap;
use bytes::Bytes;
use clap::Parser;
use egress_proto::{NodeAclConfig, NodeConfig, NodeControlPlaneConfig, NodeTlsConfig, TargetAcl};
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use prometheus::{Encoder, Histogram, IntCounterVec, IntGauge, Opts, Registry, TextEncoder};
use reqwest::Client;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::RootCertStore;
use std::convert::Infallible;
use std::fs::File;
use std::io::BufReader;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;
use tokio_rustls::TlsAcceptor;
use tracing::{info, warn};

#[derive(Debug, Parser)]
#[command(name = "su8-egress-node")]
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
    let cfg: NodeConfig = toml::from_str(&raw).context("parse node config")?;
    let acceptor = build_acceptor(&cfg.tls)?;
    let acl = if let Some(control) = &cfg.control_plane {
        let http = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .context("build control plane HTTP client")?;
        let initial = match fetch_node_acl_versioned(&http, control, None).await {
            Ok(AclFetch::Updated { acl, .. }) => acl,
            Ok(AclFetch::Gone) => {
                warn!(node = %control.node, "node missing at startup; starting with an empty ACL");
                TargetAcl::default()
            }
            Ok(AclFetch::NotModified) => TargetAcl::default(),
            Err(err) => {
                warn!(node = %control.node, error = %err, "control plane unavailable at startup; starting with an empty ACL");
                TargetAcl::default()
            }
        };
        let acl = Arc::new(ArcSwap::from_pointee(initial));
        tokio::spawn(acl_poll_loop(control.clone(), acl.clone(), http));
        acl
    } else {
        Arc::new(ArcSwap::from_pointee(cfg.acl.clone()))
    };
    let metrics = Metrics::new()?;
    let state = Arc::new(State {
        cfg: Arc::new(cfg),
        acl,
        metrics,
    });
    let listener = TcpListener::bind(state.cfg.listen).await?;
    info!(listen = %state.cfg.listen, "egress node listening");
    loop {
        let (tcp, peer) = listener.accept().await?;
        let acceptor = acceptor.clone();
        let state = state.clone();
        tokio::spawn(async move {
            match acceptor.accept(tcp).await {
                Ok(tls) => {
                    let service = service_fn(move |req| handle(req, state.clone()));
                    if let Err(err) = http1::Builder::new()
                        .serve_connection(TokioIo::new(tls), service)
                        .with_upgrades()
                        .await
                    {
                        warn!(%peer, error = %err, "connection error");
                    }
                }
                Err(err) => warn!(%peer, error = %err, "tls handshake failed"),
            }
        });
    }
}

struct State {
    cfg: Arc<NodeConfig>,
    acl: Arc<ArcSwap<TargetAcl>>,
    metrics: Metrics,
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
            if state.acl.load().allow.is_empty() {
                Ok(text(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "no allowed targets\n",
                ))
            } else {
                Ok(text(StatusCode::OK, "ok\n"))
            }
        }
        (&Method::GET, "/metrics") => Ok(metrics_response(&state.metrics)),
        _ => Ok(text(StatusCode::NOT_FOUND, "not found\n")),
    }
}

async fn handle_connect(req: Request<Incoming>, state: Arc<State>) -> Response<Full<Bytes>> {
    let Some(authority) = req.uri().authority().map(|value| value.to_string()) else {
        return text(StatusCode::BAD_REQUEST, "missing authority\n");
    };
    if !state.acl.load().allows(&authority) {
        state.metrics.connects.with_label_values(&["denied"]).inc();
        warn!(%authority, "target denied by ACL");
        return text(StatusCode::FORBIDDEN, "target not allowed\n");
    }

    let started = Instant::now();
    let connect_timeout = Duration::from_secs(state.cfg.connect_timeout_secs);
    let upstream = match timeout(connect_timeout, TcpStream::connect(&authority)).await {
        Ok(Ok(stream)) => stream,
        Ok(Err(err)) => {
            state
                .metrics
                .connects
                .with_label_values(&["upstream_error"])
                .inc();
            warn!(%authority, error = %err, "upstream connect failed");
            return text(StatusCode::BAD_GATEWAY, "upstream connect failed\n");
        }
        Err(_) => {
            state
                .metrics
                .connects
                .with_label_values(&["upstream_error"])
                .inc();
            warn!(%authority, "upstream connect timeout");
            return text(StatusCode::BAD_GATEWAY, "upstream connect timeout\n");
        }
    };
    state
        .metrics
        .upstream_seconds
        .observe(started.elapsed().as_secs_f64());
    state.metrics.active.inc();

    let on_upgrade = hyper::upgrade::on(req);
    let metrics = state.metrics.clone();
    tokio::spawn(async move {
        let _active = ActiveGuard(metrics.active.clone());
        match on_upgrade.await {
            Ok(upgraded) => {
                metrics.connects.with_label_values(&["ok"]).inc();
                let mut client = TokioIo::new(upgraded);
                let mut upstream = upstream;
                match copy_bidirectional(&mut client, &mut upstream).await {
                    Ok((up, down)) => {
                        metrics.bytes.with_label_values(&["up"]).inc_by(up);
                        metrics.bytes.with_label_values(&["down"]).inc_by(down);
                    }
                    Err(err) => warn!(%authority, error = %err, "tunnel copy failed"),
                }
            }
            Err(err) => {
                metrics.connects.with_label_values(&["upgrade_error"]).inc();
                warn!(%authority, error = %err, "upgrade failed");
            }
        }
    });

    Response::new(Full::new(Bytes::new()))
}

struct ActiveGuard(IntGauge);

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        self.0.dec();
    }
}

fn text(status: StatusCode, body: &'static str) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header("content-type", "text/plain; charset=utf-8")
        .body(Full::new(Bytes::from_static(body.as_bytes())))
        .expect("static response")
}

fn metrics_response(metrics: &Metrics) -> Response<Full<Bytes>> {
    let mut buffer = Vec::new();
    let encoder = TextEncoder::new();
    if let Err(err) = encoder.encode(&metrics.registry.gather(), &mut buffer) {
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
    active: IntGauge,
    connects: IntCounterVec,
    upstream_seconds: Histogram,
    bytes: IntCounterVec,
}

impl Metrics {
    fn new() -> Result<Self> {
        let registry = Registry::new();
        let active = IntGauge::new(
            "su8_egress_node_active_connections",
            "Active CONNECT tunnels",
        )?;
        let connects = IntCounterVec::new(
            Opts::new("su8_egress_node_connect_total", "CONNECT attempts"),
            &["result"],
        )?;
        let upstream_seconds = Histogram::with_opts(prometheus::HistogramOpts::new(
            "su8_egress_node_upstream_connect_seconds",
            "Upstream TCP connect time",
        ))?;
        let bytes = IntCounterVec::new(
            Opts::new("su8_egress_node_bytes_total", "Bytes forwarded"),
            &["direction"],
        )?;
        registry.register(Box::new(active.clone()))?;
        registry.register(Box::new(connects.clone()))?;
        registry.register(Box::new(upstream_seconds.clone()))?;
        registry.register(Box::new(bytes.clone()))?;
        Ok(Self {
            registry,
            active,
            connects,
            upstream_seconds,
            bytes,
        })
    }
}

impl Clone for Metrics {
    fn clone(&self) -> Self {
        Self {
            registry: self.registry.clone(),
            active: self.active.clone(),
            connects: self.connects.clone(),
            upstream_seconds: self.upstream_seconds.clone(),
            bytes: self.bytes.clone(),
        }
    }
}

enum AclFetch {
    NotModified,
    Updated { version: String, acl: TargetAcl },
    Gone,
}

async fn fetch_node_acl_versioned(
    http: &Client,
    control: &NodeControlPlaneConfig,
    etag: Option<&str>,
) -> Result<AclFetch> {
    let url = format!(
        "{}/internal/v1/node-config/{}",
        control.url.trim_end_matches('/'),
        urlencoding::encode(&control.node)
    );
    let mut request = http.get(&url).bearer_auth(&control.token);
    if let Some(etag) = etag {
        request = request.header(reqwest::header::IF_NONE_MATCH, format!("\"{etag}\""));
    }
    let response = request.send().await.context("control plane request")?;
    match response.status() {
        reqwest::StatusCode::NOT_MODIFIED => Ok(AclFetch::NotModified),
        reqwest::StatusCode::NOT_FOUND => Ok(AclFetch::Gone),
        status if status.is_success() => {
            let config = response
                .json::<NodeAclConfig>()
                .await
                .context("parse node ACL")?;
            Ok(AclFetch::Updated {
                version: config.version,
                acl: TargetAcl { allow: config.acl },
            })
        }
        status => bail!("control plane returned {status}"),
    }
}

async fn acl_poll_loop(
    control: NodeControlPlaneConfig,
    acl: Arc<ArcSwap<TargetAcl>>,
    http: Client,
) {
    let interval = Duration::from_secs(control.poll_interval_secs.max(1));
    let mut version: Option<String> = None;
    loop {
        match fetch_node_acl_versioned(&http, &control, version.as_deref()).await {
            Ok(AclFetch::NotModified) => {}
            Ok(AclFetch::Updated {
                version: next_version,
                acl: next_acl,
            }) => {
                version = Some(next_version);
                acl.store(Arc::new(next_acl));
                info!(node = %control.node, "node ACL updated");
            }
            Ok(AclFetch::Gone) => {
                if !acl.load().allow.is_empty() {
                    acl.store(Arc::new(TargetAcl::default()));
                    warn!(node = %control.node, "control plane reports node gone; ACL cleared");
                }
                version = None;
            }
            Err(err) => warn!(node = %control.node, error = %err, "node ACL poll failed"),
        }
        tokio::time::sleep(interval).await;
    }
}

fn build_acceptor(tls: &NodeTlsConfig) -> Result<TlsAcceptor> {
    let certs = load_certs(&tls.cert)?;
    let key = load_private_key(&tls.key)?;
    let mut roots = RootCertStore::empty();
    for cert in load_certs(&tls.client_ca)? {
        roots.add(cert).context("add client CA")?;
    }
    let verifier = rustls::server::WebPkiClientVerifier::builder(Arc::new(roots))
        .build()
        .context("build client certificate verifier")?;
    let mut config = rustls::ServerConfig::builder()
        .with_client_cert_verifier(verifier)
        .with_single_cert(certs, key)
        .context("build server TLS config")?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(TlsAcceptor::from(Arc::new(config)))
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

#[allow(dead_code)]
fn _assert_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<State>();
    assert_send_sync::<Metrics>();
    assert_send_sync::<SocketAddr>();
}
