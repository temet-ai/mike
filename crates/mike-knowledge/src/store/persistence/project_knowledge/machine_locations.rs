use uuid::Uuid;

pub use crate::store::persistence::project_placement::MachineLocation;

use crate::store::persistence::PersistError;

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub async fn project_roots_on_machine(
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
}
