use uuid::Uuid;

use super::locations::upsert_location;
use crate::store::persistence::PersistError;

pub(crate) struct RepoLocationAdoption<'a> {
    pub(crate) session: Uuid,
    pub(crate) machine: Uuid,
    pub(crate) machine_name: &'a str,
    pub(crate) checkout_root: &'a str,
    pub(crate) project_id: i64,
}

pub(crate) struct MachineLocationRoot {
    pub(crate) root_path: String,
    pub(crate) project_id: i64,
}

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub(crate) async fn machine_location_roots(
        self,
        machine: Uuid,
    ) -> Result<Vec<MachineLocationRoot>, PersistError> {
        Ok(sqlx::query_as!(
            MachineLocationRoot,
            r#"SELECT DISTINCT ON (l.root_path) l.root_path,
                    COALESCE(p.merged_into_id, p.id) AS "project_id!"
             FROM projects.locations l JOIN projects.projects p ON p.id = l.project_id
             WHERE l.machine_uuid = $1 ORDER BY l.root_path, l.last_seen DESC"#,
            machine,
        )
        .fetch_all(self.pool())
        .await?)
    }

    pub(crate) async fn project_id_for_path(self, path: &str) -> Result<Option<i64>, PersistError> {
        Ok(sqlx::query_scalar!(
            r#"SELECT COALESCE(merged_into_id, id) AS "project_id!" FROM projects.projects
             WHERE project_path = rtrim($1, '/')"#,
            path,
        )
        .fetch_optional(self.pool())
        .await?)
    }

    pub(crate) async fn adopt_repo_location(
        self,
        adoption: &RepoLocationAdoption<'_>,
    ) -> Result<Option<Uuid>, PersistError> {
        let unbound = sqlx::query_scalar!(
            r#"SELECT location_uuid IS NULL AND project_resolution <> 'operator' AS "unbound!"
             FROM sessions.sessions WHERE session_uuid = $1"#,
            adoption.session,
        )
        .fetch_optional(self.pool())
        .await?;
        if unbound != Some(true) {
            return Ok(None);
        }
        self.add_checkout_location(
            adoption.machine,
            adoption.machine_name,
            adoption.checkout_root,
            adoption.project_id,
        )
        .await
    }

    pub(crate) async fn add_checkout_location(
        self,
        machine: Uuid,
        machine_name: &str,
        checkout_root: &str,
        project_id: i64,
    ) -> Result<Option<Uuid>, PersistError> {
        let mut tx = self.pool().begin().await?;
        sqlx::query!(
            "INSERT INTO projects.machines(machine_uuid, name) VALUES ($1, $2)
             ON CONFLICT(machine_uuid) DO NOTHING",
            machine,
            machine_name
        )
        .execute(&mut *tx)
        .await?;
        let location = upsert_location(&mut tx, project_id, machine, checkout_root).await?;
        tx.commit().await?;
        Ok(location)
    }

    pub(crate) async fn set_location_origin(
        self,
        location: Uuid,
        origin: &str,
    ) -> Result<(), PersistError> {
        sqlx::query!(
            "UPDATE projects.locations SET origin_url = $2
             WHERE location_uuid = $1 AND origin_url IS DISTINCT FROM $2",
            location,
            origin,
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub(crate) async fn project_ids_for_origin(
        self,
        origin: &str,
    ) -> Result<Vec<i64>, PersistError> {
        Ok(sqlx::query_scalar!(
            r#"SELECT DISTINCT COALESCE(p.merged_into_id, p.id) AS "project_id!"
             FROM projects.locations l JOIN projects.projects p ON p.id = l.project_id
             WHERE l.origin_url = $1"#,
            origin,
        )
        .fetch_all(self.pool())
        .await?)
    }

    pub(crate) async fn project_id_enclosing(
        self,
        machine: Uuid,
        path: &str,
    ) -> Result<Option<i64>, PersistError> {
        Ok(sqlx::query_scalar!(
            r#"SELECT COALESCE(p.merged_into_id, p.id) AS "project_id!"
             FROM projects.locations l JOIN projects.projects p ON p.id = l.project_id
             WHERE l.machine_uuid = $1
               AND (rtrim(l.root_path, '/') = rtrim($2, '/')
                    OR starts_with($2, rtrim(l.root_path, '/') || '/'))
             ORDER BY length(l.root_path) DESC, l.location_uuid
             LIMIT 1"#,
            machine,
            path,
        )
        .fetch_optional(self.pool())
        .await?)
    }
}
