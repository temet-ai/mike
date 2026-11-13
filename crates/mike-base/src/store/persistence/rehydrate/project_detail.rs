use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::{ProjectDetailHead, ProjectListRow};
use crate::store::persistence::{PersistError, SENTINEL_PROJECT_PATH};

#[derive(Debug)]
pub struct ProjectLocationRow {
    pub location_uuid: Uuid,
    pub machine_uuid: Uuid,
    pub machine_name: String,
    pub root_path: String,
    pub last_seen: DateTime<Utc>,
}

pub async fn find_project_id_by_uuid(
    pool: &PgPool,
    project_uuid: Uuid,
) -> Result<Option<i64>, PersistError> {
    let id = sqlx::query_scalar!(
        "SELECT id FROM projects.projects WHERE project_uuid = $1 AND project_path <> $2",
        project_uuid,
        SENTINEL_PROJECT_PATH,
    )
    .fetch_optional(pool)
    .await?;
    Ok(id)
}

pub async fn fetch_project_row(
    pool: &PgPool,
    project_id: i64,
) -> Result<Option<ProjectDetailHead>, PersistError> {
    let row = sqlx::query!(
        r#"SELECT
             p.id AS project_id,
             p.project_uuid,
             p.project_path,
             COALESCE(sess_agg.session_count, 0) AS "session_count!",
             COALESCE(req_agg.request_count, 0)  AS "request_count!",
             p.first_seen,
             p.last_seen,
             p.pinned,
             p.ttl_override,
             p.pin_created_at,
             p.ttl_override_set_at,
             p.custom_name
         FROM projects.projects p
         LEFT JOIN (
             SELECT project_id, COUNT(*) AS session_count
             FROM sessions.sessions
             WHERE project_id = $1
             GROUP BY project_id
         ) sess_agg ON sess_agg.project_id = p.id
         LEFT JOIN (
             SELECT s.project_id, COUNT(r.id) AS request_count
             FROM sessions.sessions s
             JOIN requests.requests r ON r.session_id = s.id
             WHERE s.project_id = $1
             GROUP BY s.project_id
         ) req_agg ON req_agg.project_id = p.id
         WHERE p.id = $1 AND p.project_path <> $2"#,
        project_id,
        SENTINEL_PROJECT_PATH,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| ProjectDetailHead {
        project: ProjectListRow {
            project_id: r.project_id,
            project_uuid: r.project_uuid,
            project_path: r.project_path,
            first_seen: r.first_seen,
            last_seen: r.last_seen,
            pinned: r.pinned,
            ttl_override: r.ttl_override,
            pin_created_at: r.pin_created_at,
            ttl_override_set_at: r.ttl_override_set_at,
            custom_name: r.custom_name,
        },
        session_count: r.session_count,
        request_count: r.request_count,
    }))
}

pub async fn list_project_location_rows(
    pool: &PgPool,
    project_id: i64,
) -> Result<Vec<ProjectLocationRow>, PersistError> {
    let rows = sqlx::query_as!(
        ProjectLocationRow,
        "SELECT l.location_uuid, l.machine_uuid, m.name AS machine_name, l.root_path, l.last_seen
         FROM projects.locations l
         JOIN projects.machines m ON m.machine_uuid = l.machine_uuid
         WHERE l.project_id = $1
         ORDER BY m.name, l.root_path, l.location_uuid",
        project_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
