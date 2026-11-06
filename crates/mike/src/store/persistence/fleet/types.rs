use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct FleetRow {
    pub id: i64,
    pub fleet_uuid: Uuid,
    pub slug: String,
    pub project_path: String,
    pub state: String,
    pub definition: serde_json::Value,
    pub missing: serde_json::Value,
    pub run_id: Option<String>,
    pub start_report: Option<serde_json::Value>,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub ready_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

impl FleetRow {
    pub fn saved(
        &self,
        definition: serde_json::Value,
        missing: serde_json::Value,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            definition,
            missing,
            updated_at,
            ..self.clone()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct FleetHeaderRow {
    pub fleet_uuid: Uuid,
    pub slug: String,
    pub project_path: String,
    pub state: String,
    pub run_id: Option<String>,
    pub missing: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub ready_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

impl From<&FleetRow> for FleetHeaderRow {
    fn from(row: &FleetRow) -> Self {
        Self {
            fleet_uuid: row.fleet_uuid,
            slug: row.slug.clone(),
            project_path: row.project_path.clone(),
            state: row.state.clone(),
            run_id: row.run_id.clone(),
            missing: row.missing.clone(),
            created_at: row.created_at,
            updated_at: row.updated_at,
            ready_at: row.ready_at,
            started_at: row.started_at,
            finished_at: row.finished_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewFleet {
    pub slug: String,
    pub project_path: String,
    pub definition: serde_json::Value,
    pub missing: serde_json::Value,
    pub created_by: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FleetSeatRow {
    pub seat: Option<String>,
    pub role: Option<String>,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub session_uuid: Option<Uuid>,
    pub surface_ref: Option<String>,
    pub opened_at: Option<DateTime<Utc>>,
    pub closed_at: Option<DateTime<Utc>>,
    pub requests: i64,
    pub last_request_at: Option<DateTime<Utc>>,
    pub context_tokens: Option<i64>,
    pub last_status: Option<i16>,
    pub forked: bool,
    pub first_cache_read: Option<i64>,
    pub first_cache_written: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FleetNoteRow {
    pub note_uuid: Uuid,
    pub session_uuid: Uuid,
    pub kind: String,
    pub status: Option<String>,
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
    pub data: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub seat: Option<String>,
    pub role: Option<String>,
}
