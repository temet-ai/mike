use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::store::persistence::PersistError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct SessionObservationRow {
    pub session_uuid: Uuid,
    pub runtime: String,
    pub source: String,
    pub source_version: Option<String>,
    pub fidelity: String,
    pub completeness: String,
    pub observed_at: Option<DateTime<Utc>>,
    pub lifecycle: Option<String>,
    pub lifecycle_at: Option<DateTime<Utc>>,
    pub control: Option<String>,
    pub requested_model: Option<String>,
    pub observed_model: Option<String>,
    pub usage_scope: Option<String>,
    pub usage_basis: Option<String>,
    pub input_tokens: Option<i64>,
    pub cached_input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub reasoning_output_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub context_window: Option<i64>,
    pub model_call_count: i64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RuntimeItemRow {
    pub item_id: String,
    pub turn_id: Option<String>,
    pub kind: String,
    pub name: Option<String>,
    pub status: Option<String>,
    pub success: Option<bool>,
    pub exit_code: Option<i32>,
    pub source: String,
    pub fidelity: String,
    pub completeness: String,
    pub first_seen_at: DateTime<Utc>,
    pub observed_at: DateTime<Utc>,
    pub content_availability: String,
}

impl crate::store::persistence::runtime_repo::RuntimeDb<'_> {
    pub async fn session_observation_time(
        self,
        session: Uuid,
        observation: Uuid,
    ) -> Result<Option<DateTime<Utc>>, PersistError> {
        Ok(sqlx::query_scalar!(
            "SELECT r.observed_at FROM runtime.observation_receipts r
             JOIN sessions.sessions s ON s.id = r.session_id
             WHERE s.session_uuid = $1 AND r.observation_id = $2",
            session,
            observation
        )
        .fetch_optional(self.pool())
        .await?)
    }

    pub async fn session_observations(
        self,
        sessions: &[Uuid],
    ) -> Result<Vec<SessionObservationRow>, PersistError> {
        Ok(sqlx::query_file_as!(
            SessionObservationRow,
            "src/store/persistence/runtime/session_observations.sql",
            sessions
        )
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn list_runtime_sessions(
        self,
        runtime: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<SessionObservationRow>, PersistError> {
        Ok(sqlx::query_file_as!(
            SessionObservationRow,
            "src/store/persistence/runtime/list_runtime_sessions.sql",
            runtime,
            limit.clamp(1, 200),
            offset.max(0)
        )
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn list_runtime_items(
        self,
        session: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<RuntimeItemRow>, PersistError> {
        Ok(sqlx::query_as!(
            RuntimeItemRow,
            r#"SELECT i.item_id, i.turn_id, i.kind, i.name, i.status, i.success, i.exit_code,
                      i.source, i.fidelity, i.completeness, i.first_seen_at, i.observed_at,
                      'not_collected'::text AS "content_availability!"
               FROM runtime.session_items i JOIN sessions.sessions s ON s.id = i.session_id
               WHERE s.session_uuid = $1 ORDER BY i.first_seen_at DESC, i.id DESC LIMIT $2 OFFSET $3"#,
            session, limit.clamp(1, 200), offset.max(0)
        ).fetch_all(self.pool()).await?)
    }
}
