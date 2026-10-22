use crate::error::AppError;
use crate::models::HealthReport;
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use egress_proto::{NodeAclConfig, NodeEndpoint, RouterLineConfig};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, FromRow)]
struct InternalLine {
    id: Uuid,
    code: String,
    policy: String,
    status: String,
}

pub async fn line_config(
    Path(line_code): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let line: Option<InternalLine> =
        sqlx::query_as("select id, code, policy, status from lines where code = $1")
            .bind(&line_code)
            .fetch_optional(&state.db)
            .await?;
    let Some(line) = line else {
        return Err(AppError::not_found("line 不存在"));
    };
    if line.status != "active" {
        return Err(AppError::not_found("line 已停用"));
    }
    let rows: Vec<(String, String, String, i32)> = sqlx::query_as(
        "select n.name, n.endpoint, n.server_name, n.weight
         from line_nodes ln
         join nodes n on n.id = ln.node_id
         where ln.line_id = $1 and n.status = 'active'
         order by ln.priority, n.name",
    )
    .bind(line.id)
    .fetch_all(&state.db)
    .await?;
    let nodes: Vec<NodeEndpoint> = rows
        .into_iter()
        .map(|(name, endpoint, server_name, weight)| NodeEndpoint {
            name,
            endpoint,
            server_name,
            weight: weight.max(1) as u32,
        })
        .collect();
    let version = hash_payload(&serde_json::json!({
        "line": &line.code,
        "policy": &line.policy,
        "nodes": &nodes,
    }))?;
    let payload = RouterLineConfig {
        version: version.clone(),
        line: line.code,
        policy: line.policy,
        nodes,
    };
    let etag = format!("\"{version}\"");
    if matches_etag(&headers, &etag) {
        return not_modified(&etag);
    }
    json_with_etag(payload, &etag)
}

pub async fn node_config(
    Path(node_name): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let row: Option<(Uuid, Value)> =
        sqlx::query_as("select id, target_acl from nodes where name = $1")
            .bind(&node_name)
            .fetch_optional(&state.db)
            .await?;
    let Some((_id, acl)) = row else {
        return Err(AppError::not_found("node 不存在"));
    };
    let acl: Vec<String> = serde_json::from_value(acl).map_err(AppError::internal)?;
    let version = hash_payload(&serde_json::json!({ "acl": &acl }))?;
    let payload = NodeAclConfig {
        version: version.clone(),
        acl,
    };
    let etag = format!("\"{version}\"");
    if matches_etag(&headers, &etag) {
        return not_modified(&etag);
    }
    json_with_etag(payload, &etag)
}

pub async fn report_health(
    Path(node_name): Path<String>,
    State(state): State<AppState>,
    Json(report): Json<HealthReport>,
) -> Result<StatusCode, AppError> {
    let node: Option<(Uuid,)> = sqlx::query_as("select id from nodes where name = $1")
        .bind(&node_name)
        .fetch_optional(&state.db)
        .await?;
    let Some((node_id,)) = node else {
        return Err(AppError::not_found("node 不存在"));
    };
    sqlx::query(
        "insert into node_health (node_id, healthy, checked_at, checked_by, latency_ms, detail)
         values ($1, $2, now(), 'router', $3, $4)
         on conflict (node_id) do update set
           healthy = excluded.healthy,
           checked_at = excluded.checked_at,
           checked_by = excluded.checked_by,
           latency_ms = excluded.latency_ms,
           detail = excluded.detail",
    )
    .bind(node_id)
    .bind(report.healthy)
    .bind(report.latency_ms)
    .bind(report.detail)
    .execute(&state.db)
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

fn hash_payload<T: Serialize>(value: &T) -> Result<String, AppError> {
    let bytes = serde_json::to_vec(value).map_err(AppError::internal)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn matches_etag(headers: &HeaderMap, etag: &str) -> bool {
    headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .any(|candidate| candidate == etag || candidate == "*")
        })
        .unwrap_or(false)
}

fn json_with_etag<T: Serialize>(payload: T, etag: &str) -> Result<Response, AppError> {
    let mut response = Json(payload).into_response();
    response.headers_mut().insert(
        header::ETAG,
        HeaderValue::from_str(etag).map_err(AppError::internal)?,
    );
    Ok(response)
}

fn not_modified(etag: &str) -> Result<Response, AppError> {
    let mut response = StatusCode::NOT_MODIFIED.into_response();
    response.headers_mut().insert(
        header::ETAG,
        HeaderValue::from_str(etag).map_err(AppError::internal)?,
    );
    Ok(response)
}
