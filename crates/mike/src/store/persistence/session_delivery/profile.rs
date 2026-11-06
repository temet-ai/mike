use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::super::PersistError;

#[derive(Debug)]
pub(crate) struct DeliverySessionRow {
    pub(crate) id: i64,
    pub(crate) session_uuid: Uuid,
    pub(crate) first_seen: DateTime<Utc>,
    pub(crate) last_seen: DateTime<Utc>,
    pub(crate) project_resolution: String,
    pub(crate) project_id: i64,
    pub(crate) project_path: String,
    pub(crate) project_name: String,
}

#[derive(Debug)]
pub(crate) struct DeliveryModelRow {
    pub(crate) model: Option<String>,
    pub(crate) request_count: i64,
    pub(crate) manifest_count: i64,
}

#[derive(Debug)]
pub(crate) struct DeliveryAgentRow {
    pub(crate) persona: Option<String>,
    pub(crate) agent_name: Option<String>,
    pub(crate) first_request_id: Option<i64>,
    pub(crate) started_at: Option<DateTime<Utc>>,
}

#[derive(Debug)]
pub(crate) struct DeliveryToggleChangeRow {
    pub(crate) toggle_key: String,
    pub(crate) changed_at: DateTime<Utc>,
    pub(crate) enabled_before: Option<bool>,
    pub(crate) enabled_after: Option<bool>,
    pub(crate) via: String,
}

pub(super) async fn find_session(
    pool: &PgPool,
    session_uuid: Uuid,
) -> Result<Option<DeliverySessionRow>, PersistError> {
    let row = sqlx::query_as!(
        DeliverySessionRow,
        r#"SELECT s.id,
                  s.session_uuid,
                  s.first_seen,
                  s.last_seen,
                  s.project_resolution,
                  p.id AS project_id,
                  p.project_path,
                  COALESCE(p.custom_name, p.display_name) AS "project_name!"
             FROM sessions.sessions s
             JOIN projects.projects p ON p.id = s.project_id
            WHERE s.session_uuid = $1"#,
        session_uuid
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub(super) async fn models(
    pool: &PgPool,
    session_id: i64,
) -> Result<Vec<DeliveryModelRow>, PersistError> {
    let rows = sqlx::query_as!(
        DeliveryModelRow,
        r#"SELECT r.model,
                  count(*) AS "request_count!",
                  count(r.transformation_manifest) AS "manifest_count!"
             FROM requests.requests r
            WHERE r.session_id = $1
            GROUP BY r.model
            ORDER BY min(r.id)"#,
        session_id
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub(super) async fn agents(
    pool: &PgPool,
    session_id: i64,
) -> Result<Vec<DeliveryAgentRow>, PersistError> {
    let rows = sqlx::query_as!(
        DeliveryAgentRow,
        r#"SELECT i.subagent_persona AS persona,
                  a.name AS agent_name,
                  i.first_request_id,
                  i.started_at
             FROM agents.instances i
        LEFT JOIN agents.agents a ON a.id = i.agent_id
            WHERE i.session_id = $1
            ORDER BY i.id"#,
        session_id
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub(super) async fn toggle_changes(
    pool: &PgPool,
    since: DateTime<Utc>,
    toggle_keys: &[String],
) -> Result<Vec<DeliveryToggleChangeRow>, PersistError> {
    let rows = sqlx::query_as!(
        DeliveryToggleChangeRow,
        r#"SELECT c.subject_key AS toggle_key,
                  c.changed_at,
                  (c.old_value->>'enabled')::boolean AS enabled_before,
                  (c.new_value->>'enabled')::boolean AS enabled_after,
                  c.via
             FROM config.change_log c
            WHERE c.subject_kind = 'toggle'
              AND c.changed_at >= $1
              AND c.subject_key = ANY($2)
            ORDER BY c.changed_at, c.id"#,
        since,
        toggle_keys
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
