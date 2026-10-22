use super::common::{audit, validate_code, validate_name, validate_status, validate_weight};
use crate::auth::AuthUser;
use crate::error::AppError;
use crate::models::{LineDetail, LineInput, LineNodeView, LineView, SetLineNodes};
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use serde_json::{json, Value};
use std::collections::HashSet;
use uuid::Uuid;

const LINE_SELECT: &str = "
select l.id, l.code, l.name, l.region_id, r.code as region_code, r.name as region_name,
       l.policy, l.status, l.created_at, l.updated_at,
       (select count(*) from line_nodes ln where ln.line_id = l.id) as node_count,
       (select count(*) from line_nodes ln
          join nodes n on n.id = ln.node_id
          join node_health h on h.node_id = n.id
        where ln.line_id = l.id and n.status = 'active' and h.healthy) as healthy_node_count
from lines l
left join regions r on r.id = l.region_id
";

pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<LineView>>, AppError> {
    let query = format!("{LINE_SELECT} order by l.code");
    let rows = sqlx::query_as::<_, LineView>(&query)
        .fetch_all(&state.db)
        .await?;
    Ok(Json(rows))
}

pub async fn get(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<LineDetail>, AppError> {
    let query = format!("{LINE_SELECT} where l.id = $1");
    let line = sqlx::query_as::<_, LineView>(&query)
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("line 不存在"))?;
    let nodes = sqlx::query_as::<_, LineNodeView>(
        "select n.id, n.name, n.endpoint, n.server_name, n.status, ln.weight, ln.priority,
                h.healthy, h.checked_at, h.latency_ms
         from line_nodes ln
         join nodes n on n.id = ln.node_id
         left join node_health h on h.node_id = n.id
         where ln.line_id = $1
         order by ln.priority, n.name",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(LineDetail { line, nodes }))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(input): Json<LineInput>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let code = validate_code(&input.code, "code")?;
    let name = validate_name(&input.name, "name")?;
    let policy = validate_status(&input.policy, &["least-connections", "round-robin"])?;
    let status = validate_status(&input.status, &["active", "disabled"])?;
    ensure_region(&state, input.region_id).await?;
    let id: (Uuid,) = sqlx::query_as(
        "insert into lines (code, name, region_id, policy, status)
         values ($1, $2, $3, $4, $5) returning id",
    )
    .bind(&code)
    .bind(&name)
    .bind(input.region_id)
    .bind(policy)
    .bind(status)
    .fetch_one(&state.db)
    .await?;
    audit(
        &state.db,
        &user.username,
        "line.create",
        &code,
        json!({ "name": name, "policy": policy, "status": status }),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id.0 }))))
}

pub async fn update(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(input): Json<LineInput>,
) -> Result<Json<Value>, AppError> {
    let code = validate_code(&input.code, "code")?;
    let name = validate_name(&input.name, "name")?;
    let policy = validate_status(&input.policy, &["least-connections", "round-robin"])?;
    let status = validate_status(&input.status, &["active", "disabled"])?;
    ensure_region(&state, input.region_id).await?;
    let updated: Option<(Uuid,)> = sqlx::query_as(
        "update lines set code = $2, name = $3, region_id = $4, policy = $5,
                          status = $6, updated_at = now()
         where id = $1 returning id",
    )
    .bind(id)
    .bind(&code)
    .bind(&name)
    .bind(input.region_id)
    .bind(policy)
    .bind(status)
    .fetch_optional(&state.db)
    .await?;
    if updated.is_none() {
        return Err(AppError::not_found("line 不存在"));
    }
    audit(
        &state.db,
        &user.username,
        "line.update",
        &code,
        json!({ "name": name, "policy": policy, "status": status }),
    )
    .await?;
    Ok(Json(json!({ "id": id })))
}

pub async fn delete(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Result<StatusCode, AppError> {
    let row: Option<(String,)> = sqlx::query_as("select code from lines where id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?;
    let Some((code,)) = row else {
        return Err(AppError::not_found("line 不存在"));
    };
    sqlx::query("delete from lines where id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    audit(&state.db, &user.username, "line.delete", &code, json!({})).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn set_nodes(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(input): Json<SetLineNodes>,
) -> Result<StatusCode, AppError> {
    let row: Option<(String,)> = sqlx::query_as("select code from lines where id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?;
    let Some((code,)) = row else {
        return Err(AppError::not_found("line 不存在"));
    };
    let mut seen = HashSet::new();
    for node in &input.nodes {
        if !seen.insert(node.node_id) {
            return Err(AppError::bad_request("同一个节点不能重复分配"));
        }
        validate_weight(node.weight)?;
        if !(0..=100_000).contains(&node.priority) {
            return Err(AppError::bad_request("priority 需在 0-100000"));
        }
    }

    let mut tx = state.db.begin().await?;
    sqlx::query("delete from line_nodes where line_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    for node in &input.nodes {
        sqlx::query(
            "insert into line_nodes (line_id, node_id, weight, priority)
             values ($1, $2, $3, $4)",
        )
        .bind(id)
        .bind(node.node_id)
        .bind(node.weight)
        .bind(node.priority)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    audit(
        &state.db,
        &user.username,
        "line.set_nodes",
        &code,
        json!({ "nodes": input.nodes.len() }),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn ensure_region(state: &AppState, region_id: Option<Uuid>) -> Result<(), AppError> {
    if let Some(region_id) = region_id {
        let exists: Option<(Uuid,)> = sqlx::query_as("select id from regions where id = $1")
            .bind(region_id)
            .fetch_optional(&state.db)
            .await?;
        if exists.is_none() {
            return Err(AppError::bad_request("region 不存在"));
        }
    }
    Ok(())
}
