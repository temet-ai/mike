use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::invalid;
use crate::store::persistence::PersistError;

#[derive(Debug, Clone, Serialize)]
pub struct ProjectIdentity {
    pub id: i64,
    pub project_uuid: Uuid,
    pub project_path: String,
}

#[derive(Deserialize)]
pub struct LocationInput {
    pub project: String,
    pub machine_uuid: Uuid,
    pub machine_name: String,
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

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub async fn define_knowledge_project(
        self,
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

    pub async fn list_project_locations(self, include_all: bool) -> Result<Value, PersistError> {
        let rows = sqlx::query_scalar!(
            r#"SELECT jsonb_build_object('project_uuid', p.project_uuid,
               'project_id', p.id, 'name', COALESCE(p.custom_name, p.display_name),
               'project_path', p.project_path, 'locations',
               COALESCE(jsonb_agg(jsonb_build_object('location_uuid', l.location_uuid,
                 'machine_uuid', l.machine_uuid, 'machine_name', m.name,
                 'root_path', l.root_path, 'last_seen', l.last_seen)
                 ORDER BY m.name, l.root_path, l.location_uuid)
                 FILTER (WHERE l.location_uuid IS NOT NULL), '[]'::jsonb)) AS "project!"
             FROM projects.projects p
             LEFT JOIN projects.locations l ON l.project_id = p.id
             LEFT JOIN projects.machines m ON m.machine_uuid = l.machine_uuid
             WHERE p.project_path <> '(no project)' AND p.merged_into_id IS NULL
             GROUP BY p.id ORDER BY p.id"#,
        )
        .fetch_all(self.pool())
        .await?;
        Ok(if include_all {
            super::location_view::full_view(rows)
        } else {
            super::location_view::default_view(rows)
        })
    }

    pub async fn register_project_location(
        self,
        input: &LocationInput,
    ) -> Result<Value, PersistError> {
        if input.machine_name.trim().is_empty() || input.root_path.trim().is_empty() {
            return Err(invalid("Machine name and root path must not be empty"));
        }
        let project = self.resolve_knowledge_project(&input.project).await?;
        let mut tx = self.pool().begin().await?;
        sqlx::query!(
            "INSERT INTO projects.machines(machine_uuid, name) VALUES ($1, $2)
            ON CONFLICT(machine_uuid) DO UPDATE SET name = EXCLUDED.name",
            input.machine_uuid,
            &input.machine_name
        )
        .execute(&mut *tx)
        .await?;
        let location =
            upsert_location(&mut tx, project.id, input.machine_uuid, &input.root_path).await?;
        let location = location.ok_or_else(|| invalid("Location belongs to another project"))?;
        tx.commit().await?;
        Ok(serde_json::json!({"project_uuid": project.project_uuid,
            "location_uuid": location, "project_path": project.project_path}))
    }

    pub async fn project_for_location(
        self,
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
}
