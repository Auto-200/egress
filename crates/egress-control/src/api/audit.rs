use crate::error::AppError;
use crate::models::AuditEvent;
use crate::AppState;
use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct AuditQuery {
    pub limit: Option<i64>,
}

pub async fn list(
    State(state): State<AppState>,
    Query(query): Query<AuditQuery>,
) -> Result<Json<Vec<AuditEvent>>, AppError> {
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let rows =
        sqlx::query_as::<_, AuditEvent>("select * from audit_events order by id desc limit $1")
            .bind(limit)
            .fetch_all(&state.db)
            .await?;
    Ok(Json(rows))
}
