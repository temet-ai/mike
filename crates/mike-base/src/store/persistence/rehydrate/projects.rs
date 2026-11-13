use sqlx::PgPool;
use sqlx::postgres::types::PgInterval;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::store::persistence::PersistError;

#[derive(Debug)]
pub struct ProjectListRow {
    pub project_id: i64,
    pub project_uuid: Uuid,
    pub project_path: String,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub pinned: bool,
    pub ttl_override: Option<PgInterval>,
    pub pin_created_at: Option<DateTime<Utc>>,
    pub ttl_override_set_at: Option<DateTime<Utc>>,
    pub custom_name: Option<String>,
}

#[derive(Debug)]
pub struct ProjectDetailHead {
    pub project: ProjectListRow,
    pub session_count: i64,
    pub request_count: i64,
}

pub async fn list_projects(pool: &PgPool) -> Result<Vec<ProjectListRow>, PersistError> {
    let rows = sqlx::query_as!(
        ProjectListRow,
        "SELECT
             p.id AS project_id,
             p.project_uuid,
             p.project_path,
             p.first_seen,
             p.last_seen,
             p.pinned,
             p.ttl_override,
             p.pin_created_at,
             p.ttl_override_set_at,
             p.custom_name
         FROM projects.projects p
         WHERE p.project_path <> '(no project)' AND p.merged_into_id IS NULL
         ORDER BY p.last_seen DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[derive(Debug)]
pub struct ProjectCountsRow {
    pub project_id: i64,
    pub session_count: i64,
    pub request_count: i64,
}

pub async fn list_project_counts(
    pool: &PgPool,
) -> Result<Vec<ProjectCountsRow>, PersistError> {
    let rows = sqlx::query_as!(
        ProjectCountsRow,
        r#"SELECT COALESCE(sa.project_id, ra.project_id) AS "project_id!",
                COALESCE(sa.session_count, 0) AS "session_count!",
                COALESCE(ra.request_count, 0) AS "request_count!"
         FROM (
             SELECT project_id, COUNT(*) AS session_count
             FROM sessions.sessions
             GROUP BY project_id
         ) sa
         FULL JOIN (
             SELECT s.project_id, COUNT(r.id) AS request_count
             FROM sessions.sessions s
             JOIN requests.requests r ON r.session_id = s.id
             GROUP BY s.project_id
         ) ra ON ra.project_id = sa.project_id"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn count_projects(pool: &PgPool) -> Result<i64, PersistError> {
    let n = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!" FROM projects.projects WHERE project_path <> $1 AND merged_into_id IS NULL"#,
        crate::store::persistence::SENTINEL_PROJECT_PATH,
    )
    .fetch_one(pool)
    .await?;
    Ok(n)
}
