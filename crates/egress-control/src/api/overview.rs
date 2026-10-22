use crate::error::AppError;
use crate::models::{AuditEvent, Overview, OverviewCounts};
use crate::AppState;
use axum::extract::State;
use axum::Json;

pub async fn overview(State(state): State<AppState>) -> Result<Json<Overview>, AppError> {
    let counts = sqlx::query_as::<_, OverviewCounts>(
        "select
           (select count(*) from regions) as regions,
           (select count(*) from lines) as lines,
           (select count(*) from nodes) as nodes,
           (select count(*) from node_health h
              join nodes n on n.id = h.node_id
             where h.healthy and n.status = 'active') as healthy_nodes,
           (select count(*) from nodes n
              left join node_health h on h.node_id = n.id
             where n.status = 'active' and (h.healthy is not true)) as unhealthy_nodes,
           (select count(*) from lines where status = 'active') as active_lines",
    )
    .fetch_one(&state.db)
    .await?;
    let recent_audit =
        sqlx::query_as::<_, AuditEvent>("select * from audit_events order by id desc limit 8")
            .fetch_all(&state.db)
            .await?;
    Ok(Json(Overview {
        counts,
        recent_audit,
    }))
}
