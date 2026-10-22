use super::common::{audit, validate_code, validate_name, validate_status};
use crate::auth::AuthUser;
use crate::error::AppError;
use crate::models::{Region, RegionInput};
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use serde_json::json;
use uuid::Uuid;

pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<Region>>, AppError> {
    let rows = sqlx::query_as::<_, Region>("select * from regions order by code")
        .fetch_all(&state.db)
        .await?;
    Ok(Json(rows))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(input): Json<RegionInput>,
) -> Result<(StatusCode, Json<Region>), AppError> {
    let code = validate_code(&input.code, "code")?;
    let name = validate_name(&input.name, "name")?;
    let status = validate_status(
        input.status.as_deref().unwrap_or("active"),
        &["active", "disabled"],
    )?;
    let row = sqlx::query_as::<_, Region>(
        "insert into regions (code, name, country, status) values ($1, $2, $3, $4) returning *",
    )
    .bind(&code)
    .bind(&name)
    .bind(&input.country)
    .bind(status)
    .fetch_one(&state.db)
    .await?;
    audit(
        &state.db,
        &user.username,
        "region.create",
        &row.code,
        json!({ "name": row.name }),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(row)))
}

pub async fn update(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(input): Json<RegionInput>,
) -> Result<Json<Region>, AppError> {
    let code = validate_code(&input.code, "code")?;
    let name = validate_name(&input.name, "name")?;
    let status = validate_status(
        input.status.as_deref().unwrap_or("active"),
        &["active", "disabled"],
    )?;
    let row = sqlx::query_as::<_, Region>(
        "update regions set code = $2, name = $3, country = $4, status = $5
         where id = $1 returning *",
    )
    .bind(id)
    .bind(&code)
    .bind(&name)
    .bind(&input.country)
    .bind(status)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("region 不存在"))?;
    audit(
        &state.db,
        &user.username,
        "region.update",
        &row.code,
        json!({ "name": row.name, "status": row.status }),
    )
    .await?;
    Ok(Json(row))
}

pub async fn delete(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Result<StatusCode, AppError> {
    let code: Option<(String,)> = sqlx::query_as("select code from regions where id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?;
    let Some((code,)) = code else {
        return Err(AppError::not_found("region 不存在"));
    };
    let refs: (i64, i64) = sqlx::query_as(
        "select
           (select count(*) from nodes where region_id = $1),
           (select count(*) from lines where region_id = $1)",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;
    if refs.0 > 0 || refs.1 > 0 {
        return Err(AppError::conflict("该地区仍被节点或线路引用"));
    }
    sqlx::query("delete from regions where id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    audit(&state.db, &user.username, "region.delete", &code, json!({})).await?;
    Ok(StatusCode::NO_CONTENT)
}
