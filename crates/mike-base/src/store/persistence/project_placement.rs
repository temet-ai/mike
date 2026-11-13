use uuid::Uuid;

use crate::store::persistence::project_identity::{ProjectIdentity, invalid};
use crate::store::persistence::{PersistError, PersistHandle};

pub struct RepoLocationAdoption<'a> {
    pub session: Uuid,
    pub machine: Uuid,
    pub machine_name: &'a str,
    pub checkout_root: &'a str,
    pub project_id: i64,
}

pub struct MachineLocationRoot {
    pub root_path: String,
    pub project_id: i64,
}

impl PersistHandle {
    pub async fn machine_location_roots(
        &self,
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

    pub async fn project_id_for_path(&self, path: &str) -> Result<Option<i64>, PersistError> {
        Ok(sqlx::query_scalar!(
            r#"SELECT COALESCE(merged_into_id, id) AS "project_id!" FROM projects.projects
             WHERE project_path = rtrim($1, '/')"#,
            path,
        )
        .fetch_optional(self.pool())
        .await?)
    }

    pub async fn adopt_repo_location(
        &self,
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

    pub async fn add_checkout_location(
        &self,
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

    pub async fn set_location_origin(
        &self,
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

    pub async fn project_ids_for_origin(&self, origin: &str) -> Result<Vec<i64>, PersistError> {
        Ok(sqlx::query_scalar!(
            r#"SELECT DISTINCT COALESCE(p.merged_into_id, p.id) AS "project_id!"
             FROM projects.locations l JOIN projects.projects p ON p.id = l.project_id
             WHERE l.origin_url = $1"#,
            origin,
        )
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn project_id_enclosing(
        &self,
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
    pub async fn define_knowledge_project(
        &self,
        name: &str,
        uuid: Uuid,
    ) -> Result<ProjectIdentity, PersistError> {
        if name.trim().is_empty() {
            return Err(invalid("Project name must not be empty"));
        }
        let row = sqlx::query_as!(
            ProjectIdentity,
            "INSERT INTO projects.projects(project_uuid, project_path, display_name, custom_name)
             VALUES ($1, $2, $3, $3) ON CONFLICT(project_uuid)
             DO UPDATE SET last_seen = now()
             RETURNING id, project_uuid, project_path",
            uuid,
            format!("mike:{uuid}"),
            name,
        )
        .fetch_one(self.pool())
        .await?;
        Ok(row)
    }

    pub async fn locations_on_machine(
        &self,
        machine: Uuid,
    ) -> Result<Vec<MachineLocation>, PersistError> {
        Ok(sqlx::query_as!(
            MachineLocation,
            "SELECT location_uuid, root_path FROM projects.locations
              WHERE machine_uuid = $1 ORDER BY root_path",
            machine
        )
        .fetch_all(self.pool())
        .await?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct MachineLocation {
    pub location_uuid: Uuid,
    pub root_path: String,
}

pub async fn upsert_location(
    connection: &mut sqlx::PgConnection,
    project_id: i64,
    machine: Uuid,
    root_path: &str,
) -> Result<Option<Uuid>, PersistError> {
    Ok(sqlx::query_scalar!(
        "INSERT INTO projects.locations(project_id, machine_uuid, root_path)
         VALUES ($1, $2, $3) ON CONFLICT(machine_uuid, root_path)
         DO UPDATE SET last_seen = now()
         WHERE projects.locations.project_id = EXCLUDED.project_id
         RETURNING location_uuid",
        project_id,
        machine,
        root_path,
    )
    .fetch_optional(connection)
    .await?)
}
