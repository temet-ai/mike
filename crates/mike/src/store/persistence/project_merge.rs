use serde::Serialize;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use super::{PersistError, PersistHandle, SENTINEL_PROJECT_PATH};

#[derive(Debug, Serialize)]
pub(crate) struct ProjectMerge {
    pub(crate) source_path: String,
    pub(crate) target_uuid: Uuid,
    pub(crate) target_path: String,
    pub(crate) sessions_moved: i64,
}

#[derive(Debug)]
pub(crate) enum ProjectMergeError {
    UnknownSource,
    UnknownTarget,
    SameProject,
    Db(PersistError),
}

impl From<sqlx::Error> for ProjectMergeError {
    fn from(error: sqlx::Error) -> Self {
        Self::Db(PersistError::Database(error))
    }
}

impl PersistHandle {
    pub(crate) async fn merge_project_into(
        &self,
        source_path: &str,
        target_uuid: Uuid,
    ) -> Result<ProjectMerge, ProjectMergeError> {
        let mut tx = self.pool().begin().await?;
        let source = sqlx::query_scalar!(
            "SELECT id FROM projects.projects WHERE project_path = $1 AND project_path <> $2
             FOR UPDATE",
            source_path,
            SENTINEL_PROJECT_PATH,
        )
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ProjectMergeError::UnknownSource)?;
        let resolved = sqlx::query!(
            r#"SELECT COALESCE(t.id, p.id) AS "id!", COALESCE(t.project_path, p.project_path) AS "project_path!"
             FROM projects.projects p LEFT JOIN projects.projects t ON t.id = p.merged_into_id
             WHERE p.project_uuid = $1 AND p.project_path <> $2"#,
            target_uuid,
            SENTINEL_PROJECT_PATH,
        )
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ProjectMergeError::UnknownTarget)?;
        let (target, target_path) = (resolved.id, resolved.project_path);
        if source == target {
            return Err(ProjectMergeError::SameProject);
        }
        let sessions_moved = move_project_rows(&mut tx, source, target).await?;
        mark_merged(&mut tx, source, target).await?;
        tx.commit().await?;
        self.forget_cached_projects();
        Ok(ProjectMerge {
            source_path: source_path.to_string(),
            target_uuid,
            target_path,
            sessions_moved,
        })
    }
}

async fn move_project_rows(
    tx: &mut Transaction<'_, Postgres>,
    source: i64,
    target: i64,
) -> Result<i64, sqlx::Error> {
    let sessions = sqlx::query!(
        "UPDATE sessions.sessions SET project_id = $2, project_resolution = 'operator'
         WHERE project_id = $1",
        source,
        target
    )
    .execute(&mut **tx)
    .await?
    .rows_affected();
    sqlx::query!(
        "WITH
    t0 AS (UPDATE requests.requests SET project_id = $2 WHERE project_id = $1),
    t1 AS (UPDATE memory.operator_turns SET project_id = $2 WHERE project_id = $1),
    t2 AS (UPDATE memory.lessons SET project_id = $2 WHERE project_id = $1),
    t3 AS (UPDATE memory.directive_lessons SET project_id = $2 WHERE project_id = $1),
    t4 AS (UPDATE memory.directive_proposals SET project_id = $2 WHERE project_id = $1),
    t5 AS (UPDATE memory.lesson_file_changes SET project_id = $2 WHERE project_id = $1),
    t6 AS (UPDATE memory.consolidation_actions SET project_id = $2 WHERE project_id = $1),
    t7 AS (UPDATE memory.semantic_items SET project_id = $2 WHERE project_id = $1),
    t8 AS (UPDATE workspace.session_registrations SET project_id = $2 WHERE project_id = $1),
    t9 AS (UPDATE workspace.coordination_events SET project_id = $2 WHERE project_id = $1),
    t10 AS (UPDATE steering.steerers SET project_id = $2 WHERE project_id = $1),
    t11 AS (UPDATE mailbox.threads SET project_id = $2 WHERE project_id = $1),
    t12 AS (UPDATE projects.locations SET project_id = $2 WHERE project_id = $1)
UPDATE work.threads s SET project_id = $2 WHERE s.project_id = $1
    AND NOT EXISTS (SELECT 1 FROM work.threads t WHERE t.project_id = $2 AND t.slug = s.slug)",
        source,
        target
    )
    .execute(&mut **tx)
    .await?;
    Ok(i64::try_from(sessions).unwrap_or(i64::MAX))
}

async fn mark_merged(
    tx: &mut Transaction<'_, Postgres>,
    source: i64,
    target: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE projects.projects SET merged_into_id = $2, merged_at = now()
         WHERE id = $1 OR merged_into_id = $1",
        source,
        target
    )
    .execute(&mut **tx)
    .await?;
    sqlx::query!(
        "UPDATE projects.projects t
         SET first_seen = LEAST(t.first_seen, s.first_seen),
             last_seen = GREATEST(t.last_seen, s.last_seen)
         FROM projects.projects s WHERE t.id = $2 AND s.id = $1",
        source,
        target
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[cfg(test)]
#[path = "project_merge_db_tests.rs"]
mod db_tests;
