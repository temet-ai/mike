use uuid::Uuid;

use serde::Serialize;

use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, Serialize)]
pub struct ProjectIdentity {
    pub id: i64,
    pub project_uuid: Uuid,
    pub project_path: String,
}

pub(crate) fn invalid(message: &str) -> PersistError {
    PersistError::DataInvariant(message.to_string())
}

impl PersistHandle {
    pub async fn project_for_location(
        &self,
        location: Uuid,
    ) -> Result<ProjectIdentity, PersistError> {
        sqlx::query_as!(
            ProjectIdentity,
            "SELECT p.id, p.project_uuid, p.project_path FROM projects.projects p
             JOIN projects.locations l ON l.project_id = p.id WHERE l.location_uuid = $1",
            location,
        )
        .fetch_optional(self.pool())
        .await?
        .ok_or_else(|| invalid("Unknown project location"))
    }

    pub async fn bind_session_location(
        &self,
        session: Uuid,
        location: Uuid,
    ) -> Result<ProjectIdentity, PersistError> {
        let project = self.project_for_location(location).await?;
        let mut tx = self.pool().begin().await?;
        let bound = sqlx::query!(
            r#"SELECT s.id, s.location_uuid, s.project_id AS bound_project, l.project_id AS "bound_owner?"
                 FROM sessions.sessions s
                 LEFT JOIN projects.locations l ON l.location_uuid = s.location_uuid
                 WHERE s.session_uuid = $1 FOR NO KEY UPDATE OF s"#,
            session,
        )
        .fetch_one(&mut *tx)
        .await?;
        let (id, bound_location, bound_project, bound_owner) = (
            bound.id,
            bound.location_uuid,
            bound.bound_project,
            bound.bound_owner,
        );
        if bound_location == Some(location) && bound_project == project.id {
            return Ok(project);
        }
        if bound_owner.is_some_and(|owner| owner != project.id) {
            return Err(invalid(
                "Session is already bound to a location of another project; its binding is unchanged",
            ));
        }
        super::session_projects::repoint_session_project(&mut tx, id, project.id, "operator")
            .await?;
        sqlx::query!(
            "UPDATE sessions.sessions SET location_uuid = $2 WHERE id = $1",
            id,
            location
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(project)
    }

    pub async fn bind_session_to_local_root(
        &self,
        session: Uuid,
        machine: Uuid,
        cwd: &str,
    ) -> Result<Option<ProjectIdentity>, PersistError> {
        let location = sqlx::query_scalar!(
            "SELECT l.location_uuid
             FROM sessions.sessions s
             JOIN projects.locations l ON l.machine_uuid = $2
             WHERE s.session_uuid = $1
               AND s.location_uuid IS NULL
               AND s.project_resolution <> 'operator'
               AND (rtrim(l.root_path, '/') = rtrim($3, '/')
                    OR starts_with($3, rtrim(l.root_path, '/') || '/'))
             ORDER BY length(l.root_path) DESC, l.location_uuid
             LIMIT 1",
            session,
            machine,
            cwd,
        )
        .fetch_optional(self.pool())
        .await?;
        match location {
            Some(location) => self
                .bind_session_location(session, location)
                .await
                .map(Some),
            None => Ok(None),
        }
    }
}

impl PersistHandle {
    pub async fn get_project_ids_for_path(
        &self,
        project_path: &str,
    ) -> Result<Vec<i64>, PersistError> {
        let candidates = project_path_candidates(project_path);
        crate::metrics::time_db("get_project_ids_for_path", async move {
            let ids = sqlx::query_scalar!(
                r#"SELECT id AS "id!" FROM projects.projects
                    WHERE project_path = ANY($1::text[])
                    ORDER BY array_position($1::text[], project_path)"#,
                &candidates,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(ids)
        })
        .await
    }
}

pub fn project_path_candidates(project_path: &str) -> Vec<String> {
    let mut candidates = vec![project_path.to_owned()];
    if let Some(alias) = crate::extract::request::legacy_trimmed_alias(project_path) {
        candidates.push(alias);
    }
    candidates
}
