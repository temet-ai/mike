use sqlx::{PgConnection, Postgres, Transaction};

use super::PersistHandle;
use super::error::PersistError;

#[derive(Debug, Clone)]
pub struct SessionProjectAssignment {
    pub project_id: i64,
    pub project_path: String,
    pub cwd: Option<String>,
}

#[derive(Debug)]
pub enum AssignSessionProjectError {
    UnknownSession,
    Db(PersistError),
}

impl From<PersistError> for AssignSessionProjectError {
    fn from(error: PersistError) -> Self {
        Self::Db(error)
    }
}

impl From<sqlx::Error> for AssignSessionProjectError {
    fn from(error: sqlx::Error) -> Self {
        Self::Db(PersistError::Database(error))
    }
}

pub async fn upsert_canonical_project(
    connection: &mut PgConnection,
    project_path: &str,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"INSERT INTO projects.projects
             (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, NOW(), NOW())
         ON CONFLICT (project_path) DO UPDATE
             SET last_seen = GREATEST(projects.projects.last_seen, EXCLUDED.last_seen)
         RETURNING COALESCE(merged_into_id, id) AS "id!""#,
        project_path,
    )
    .fetch_one(connection)
    .await
}

pub async fn repoint_session_project(
    tx: &mut Transaction<'_, Postgres>,
    session_id: i64,
    project_id: i64,
    resolution: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE sessions.sessions s SET location_uuid = NULL
        FROM projects.locations l WHERE s.id = $1 AND s.location_uuid = l.location_uuid
          AND l.project_id <> $2",
        session_id,
        project_id
    )
    .execute(&mut **tx)
    .await?;
    repoint_session_and_requests(tx, session_id, project_id, resolution).await?;
    repoint_remaining_project_copies(tx, session_id, project_id).await?;
    Ok(())
}

async fn repoint_session_and_requests(
    tx: &mut Transaction<'_, Postgres>,
    session_id: i64,
    project_id: i64,
    resolution: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE sessions.sessions
         SET project_id = $2, project_resolution = $3
         WHERE id = $1
           AND (project_id, project_resolution) IS DISTINCT FROM ($2, $3)",
        session_id,
        project_id,
        resolution,
    )
    .execute(&mut **tx)
    .await?;

    sqlx::query!(
        "UPDATE requests.requests SET project_id = $2
          WHERE session_id = $1 AND project_id IS DISTINCT FROM $2",
        session_id,
        project_id,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn repoint_remaining_project_copies(
    tx: &mut Transaction<'_, Postgres>,
    session_id: i64,
    project_id: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE memory.operator_turns SET project_id = $2
          WHERE session_id = $1 AND project_id IS DISTINCT FROM $2",
        session_id,
        project_id,
    )
    .execute(&mut **tx)
    .await?;

    sqlx::query!(
        "UPDATE memory.lessons SET project_id = $2
          WHERE session_id = $1 AND project_id IS DISTINCT FROM $2",
        session_id,
        project_id,
    )
    .execute(&mut **tx)
    .await?;

    sqlx::query!(
        "UPDATE workspace.session_registrations SET project_id = $2
          WHERE session_id = $1 AND project_id IS DISTINCT FROM $2",
        session_id,
        project_id,
    )
    .execute(&mut **tx)
    .await?;

    sqlx::query!(
        "UPDATE workspace.coordination_events SET project_id = $2
          WHERE session_id = $1 AND project_id IS DISTINCT FROM $2",
        session_id,
        project_id,
    )
    .execute(&mut **tx)
    .await?;

    Ok(())
}

impl PersistHandle {
    pub async fn assign_session_project(
        &self,
        session_uuid: uuid::Uuid,
        target_project_path: &str,
    ) -> Result<SessionProjectAssignment, AssignSessionProjectError> {
        let target = target_project_path.trim();
        if target.is_empty() {
            return Err(AssignSessionProjectError::Db(PersistError::Database(
                sqlx::Error::Protocol("target project path must not be empty".to_string()),
            )));
        }

        crate::metrics::time_db("assign_session_project", async move {
            let mut tx = self.pool.begin().await?;
            let session = sqlx::query!(
                "SELECT id, cwd FROM sessions.sessions WHERE session_uuid = $1",
                session_uuid,
            )
            .fetch_optional(&mut *tx)
            .await?;
            let Some(session) = session else {
                tx.rollback().await?;
                return Err(AssignSessionProjectError::UnknownSession);
            };
            let (session_id, cwd) = (session.id, session.cwd);

            let project_id = upsert_canonical_project(&mut tx, target).await?;
            repoint_session_project(&mut tx, session_id, project_id, "operator").await?;
            tx.commit().await?;

            Ok(SessionProjectAssignment {
                project_id,
                project_path: target.to_string(),
                cwd,
            })
        })
        .await
    }
}
