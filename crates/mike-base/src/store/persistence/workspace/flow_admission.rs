use super::RegisterWorkspaceSession;
use crate::store::persistence::{PersistError, SENTINEL_PROJECT_PATH};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct FlowAdmissionEvidence {
    pub capture_version: i32,
    pub admitted_at: DateTime<Utc>,
    pub producer: String,
    pub hook_event_name: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowAdmissionResult {
    pub evidence: Option<FlowAdmissionEvidence>,
}

pub async fn resolve_registration_session(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    input: &RegisterWorkspaceSession<'_>,
) -> Result<(i64, FlowAdmissionResult), PersistError> {
    if let Some(row) = existing_session(tx, input.session_uuid).await? {
        return Ok((row.id, row.result()));
    }
    let project_id = sentinel_project(tx).await?;
    let row = sqlx::query_as!(
        AdmissionRow,
        "INSERT INTO sessions.sessions (session_uuid, project_id)
         VALUES ($1, $2)
         ON CONFLICT (session_uuid) DO SELECT FOR UPDATE
         RETURNING id, flow_capture_version, flow_admitted_at, flow_producer,
                   flow_hook_event_name, flow_start_source",
        input.session_uuid,
        project_id,
    )
    .fetch_one(&mut **tx)
    .await?;
    Ok((row.id, row.result()))
}

struct AdmissionRow {
    id: i64,
    flow_capture_version: Option<i32>,
    flow_admitted_at: Option<DateTime<Utc>>,
    flow_producer: Option<String>,
    flow_hook_event_name: Option<String>,
    flow_start_source: Option<String>,
}

impl AdmissionRow {
    fn result(self) -> FlowAdmissionResult {
        let evidence = match (
            self.flow_capture_version,
            self.flow_admitted_at,
            self.flow_producer,
            self.flow_hook_event_name,
            self.flow_start_source,
        ) {
            (
                Some(capture_version),
                Some(admitted_at),
                Some(producer),
                Some(hook_event_name),
                Some(source),
            ) => Some(FlowAdmissionEvidence {
                capture_version,
                admitted_at,
                producer,
                hook_event_name,
                source,
            }),
            _ => None,
        };
        FlowAdmissionResult { evidence }
    }
}

async fn existing_session(
    tx: &mut sqlx::PgConnection,
    session: uuid::Uuid,
) -> Result<Option<AdmissionRow>, PersistError> {
    Ok(sqlx::query_as!(
        AdmissionRow,
        "SELECT id, flow_capture_version, flow_admitted_at, flow_producer,
                flow_hook_event_name, flow_start_source
         FROM sessions.sessions WHERE session_uuid = $1 FOR UPDATE",
        session,
    )
    .fetch_optional(tx)
    .await?)
}

async fn sentinel_project(tx: &mut sqlx::PgConnection) -> Result<i64, PersistError> {
    Ok(sqlx::query_scalar!(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, now(), now()) ON CONFLICT (project_path) DO SELECT FOR KEY SHARE RETURNING id",
        SENTINEL_PROJECT_PATH,
    )
    .fetch_one(&mut *tx)
    .await?)
}
