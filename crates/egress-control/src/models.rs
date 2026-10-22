use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Region {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub country: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct RegionInput {
    pub code: String,
    pub name: String,
    pub country: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct NodeView {
    pub id: Uuid,
    pub region_id: Uuid,
    pub region_code: String,
    pub region_name: String,
    pub name: String,
    pub endpoint: String,
    pub server_name: String,
    pub transport: String,
    pub weight: i32,
    pub capacity: i32,
    pub target_acl: serde_json::Value,
    pub status: String,
    pub cert_fingerprint: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub healthy: Option<bool>,
    pub checked_at: Option<DateTime<Utc>>,
    pub latency_ms: Option<i32>,
    pub detail: Option<String>,
    pub line_count: i64,
}

#[derive(Debug, Deserialize)]
pub struct NodeInput {
    pub region_id: Uuid,
    pub name: String,
    pub endpoint: String,
    pub server_name: String,
    #[serde(default = "default_transport")]
    pub transport: String,
    #[serde(default = "default_weight")]
    pub weight: i32,
    #[serde(default)]
    pub capacity: i32,
    #[serde(default)]
    pub target_acl: Vec<String>,
    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default)]
    pub cert_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct LineView {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub region_id: Option<Uuid>,
    pub region_code: Option<String>,
    pub region_name: Option<String>,
    pub policy: String,
    pub status: String,
    pub node_count: i64,
    pub healthy_node_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct LineInput {
    pub code: String,
    pub name: String,
    #[serde(default)]
    pub region_id: Option<Uuid>,
    #[serde(default = "default_policy")]
    pub policy: String,
    #[serde(default = "default_status")]
    pub status: String,
}

#[derive(Debug, Serialize, FromRow)]
pub struct LineNodeView {
    pub id: Uuid,
    pub name: String,
    pub endpoint: String,
    pub server_name: String,
    pub status: String,
    pub weight: i32,
    pub priority: i32,
    pub healthy: Option<bool>,
    pub checked_at: Option<DateTime<Utc>>,
    pub latency_ms: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct LineNodeInput {
    pub node_id: Uuid,
    #[serde(default = "default_weight")]
    pub weight: i32,
    #[serde(default = "default_priority")]
    pub priority: i32,
}

#[derive(Debug, Deserialize)]
pub struct SetLineNodes {
    #[serde(default)]
    pub nodes: Vec<LineNodeInput>,
}

#[derive(Debug, Serialize)]
pub struct LineDetail {
    pub line: LineView,
    pub nodes: Vec<LineNodeView>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct AuditEvent {
    pub id: i64,
    pub actor: String,
    pub action: String,
    pub object: String,
    pub detail: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct OverviewCounts {
    pub regions: i64,
    pub lines: i64,
    pub nodes: i64,
    pub healthy_nodes: i64,
    pub unhealthy_nodes: i64,
    pub active_lines: i64,
}

#[derive(Debug, Serialize)]
pub struct Overview {
    #[serde(flatten)]
    pub counts: OverviewCounts,
    pub recent_audit: Vec<AuditEvent>,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub user: String,
}

#[derive(Debug, Deserialize)]
pub struct HealthReport {
    pub healthy: bool,
    #[serde(default)]
    pub latency_ms: Option<i32>,
    #[serde(default)]
    pub detail: Option<String>,
}

fn default_transport() -> String {
    "https-connect".to_string()
}

fn default_weight() -> i32 {
    1
}

fn default_priority() -> i32 {
    100
}

fn default_status() -> String {
    "active".to_string()
}

fn default_policy() -> String {
    "least-connections".to_string()
}
