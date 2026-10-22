use super::common::{
    audit, validate_acl, validate_capacity, validate_endpoint, validate_name, validate_server_name,
    validate_status, validate_weight,
};
use crate::auth::AuthUser;
use crate::error::AppError;
use crate::models::{NodeInput, NodeView};
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use serde_json::{json, Value};
use uuid::Uuid;

const NODE_SELECT: &str = "
select n.id, n.region_id, r.code as region_code, r.name as region_name,
       n.name, n.endpoint, n.server_name, n.transport, n.weight, n.capacity,
       n.target_acl, n.status, n.cert_fingerprint, n.created_at, n.updated_at,
       h.healthy, h.checked_at, h.latency_ms, h.detail,
       (select count(*) from line_nodes ln where ln.node_id = n.id) as line_count
from nodes n
join regions r on r.id = n.region_id
left join node_health h on h.node_id = n.id
";

pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<NodeView>>, AppError> {
    let query = format!("{NODE_SELECT} order by r.code, n.name");
    let rows = sqlx::query_as::<_, NodeView>(&query)
        .fetch_all(&state.db)
        .await?;
    Ok(Json(rows))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(input): Json<NodeInput>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let (name, endpoint, server_name, acl) = validate_input(&state, &input).await?;
    let status = validate_status(&input.status, &["active", "disabled", "draining"])?;
    let id: (Uuid,) = sqlx::query_as(
        "insert into nodes
           (region_id, name, endpoint, server_name, transport, weight, capacity, target_acl, status, cert_fingerprint)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         returning id",
    )
    .bind(input.region_id)
    .bind(&name)
    .bind(&endpoint)
    .bind(&server_name)
    .bind(&input.transport)
    .bind(input.weight)
    .bind(input.capacity)
    .bind(acl)
    .bind(status)
    .bind(&input.cert_fingerprint)
    .fetch_one(&state.db)
    .await?;
    audit(
        &state.db,
        &user.username,
        "node.create",
        &name,
        json!({ "endpoint": endpoint, "status": status }),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id.0 }))))
}

pub async fn update(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(input): Json<NodeInput>,
) -> Result<Json<Value>, AppError> {
    let (name, endpoint, server_name, acl) = validate_input(&state, &input).await?;
    let status = validate_status(&input.status, &["active", "disabled", "draining"])?;
    let updated: Option<(Uuid,)> = sqlx::query_as(
        "update nodes set
           region_id = $2, name = $3, endpoint = $4, server_name = $5, transport = $6,
           weight = $7, capacity = $8, target_acl = $9, status = $10,
           cert_fingerprint = $11, updated_at = now()
         where id = $1
         returning id",
    )
    .bind(id)
    .bind(input.region_id)
    .bind(&name)
    .bind(&endpoint)
    .bind(&server_name)
    .bind(&input.transport)
    .bind(input.weight)
    .bind(input.capacity)
    .bind(acl)
    .bind(status)
    .bind(&input.cert_fingerprint)
    .fetch_optional(&state.db)
    .await?;
    if updated.is_none() {
        return Err(AppError::not_found("node 不存在"));
    }
    audit(
        &state.db,
        &user.username,
        "node.update",
        &name,
        json!({ "endpoint": endpoint, "status": status }),
    )
    .await?;
    Ok(Json(json!({ "id": id })))
}

pub async fn delete(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Result<StatusCode, AppError> {
    let row: Option<(String,)> = sqlx::query_as("select name from nodes where id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?;
    let Some((name,)) = row else {
        return Err(AppError::not_found("node 不存在"));
    };
    sqlx::query("delete from nodes where id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    audit(&state.db, &user.username, "node.delete", &name, json!({})).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn validate_input(
    state: &AppState,
    input: &NodeInput,
) -> Result<(String, String, String, Value), AppError> {
    let name = validate_name(&input.name, "name")?;
    let endpoint = validate_endpoint(&input.endpoint)?;
    let server_name = validate_server_name(&input.server_name)?;
    validate_weight(input.weight)?;
    validate_capacity(input.capacity)?;
    if input.transport != "https-connect" {
        return Err(AppError::bad_request("transport 目前只支持 https-connect"));
    }
    let acl = validate_acl(&input.target_acl)?;
    let region_exists: Option<(Uuid,)> = sqlx::query_as("select id from regions where id = $1")
        .bind(input.region_id)
        .fetch_optional(&state.db)
        .await?;
    if region_exists.is_none() {
        return Err(AppError::bad_request("region 不存在"));
    }
    let acl = Value::Array(acl.into_iter().map(Value::String).collect());
    Ok((name, endpoint, server_name, acl))
}
