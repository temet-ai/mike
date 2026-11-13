use uuid::Uuid;

use crate::store::persistence::PersistError;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub(crate) struct MachineLocation {
    pub location_uuid: Uuid,
    pub root_path: String,
}

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub(crate) async fn project_roots_on_machine(
        self,
        project_id: i64,
        machine: Uuid,
    ) -> Result<Vec<String>, PersistError> {
        let rows = sqlx::query!(
            "SELECT root_path FROM projects.locations
              WHERE project_id = $1 AND machine_uuid = $2
              ORDER BY last_seen DESC",
            project_id,
            machine
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|row| row.root_path).collect())
    }

    pub(crate) async fn locations_on_machine(
        self,
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
