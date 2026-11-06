use serde_json::Value;
use uuid::Uuid;

use super::invalid;
use crate::store::persistence::{PersistError, PersistHandle};

fn require_source(touched: u64) -> Result<(), PersistError> {
    if touched == 0 {
        return Err(invalid("Unknown knowledge source"));
    }
    Ok(())
}

impl PersistHandle {
    pub async fn record_knowledge_source_heartbeats(
        &self,
        sources: &[Uuid],
    ) -> Result<u64, PersistError> {
        if sources.is_empty() {
            return Ok(0);
        }
        Ok(sqlx::query!(
            "UPDATE memory.knowledge_sources SET last_heartbeat_at = now()
             WHERE source_uuid = ANY($1)
               AND (last_heartbeat_at IS NULL OR last_heartbeat_at < now() - interval '5 minutes')",
            sources
        )
        .execute(self.pool())
        .await?
        .rows_affected())
    }

    pub async fn record_knowledge_source_failure(
        &self,
        source: Uuid,
        error: &str,
    ) -> Result<(), PersistError> {
        if error.trim().is_empty() {
            return Err(invalid("A failure report must carry the error text"));
        }
        let touched = sqlx::query!(
            "UPDATE memory.knowledge_sources SET last_failure = $2, last_failure_at = now(),
                 last_failure_generation = generation, last_heartbeat_at = now()
             WHERE source_uuid = $1",
            source,
            error
        )
        .execute(self.pool())
        .await?
        .rows_affected();
        require_source(touched)
    }

    pub async fn record_lesson_file_conflicts(
        &self,
        location: Uuid,
        lessons: &[Uuid],
    ) -> Result<u64, PersistError> {
        let mut unique = lessons.to_vec();
        unique.sort();
        unique.dedup();
        self.project_for_location(location).await?;
        let recorded = sqlx::query!(
            "INSERT INTO memory.lesson_file_conflicts(location_uuid, lesson_uuid, server_version)
             SELECT $1, c.lesson_uuid, c.version
               FROM unnest($2::uuid[]) AS x(lesson_uuid)
               JOIN memory.lesson_file_changes c ON c.lesson_uuid = x.lesson_uuid
               JOIN projects.locations l
                 ON l.location_uuid = $1 AND l.project_id = c.project_id
             ON CONFLICT(location_uuid, lesson_uuid) DO UPDATE
               SET server_version = EXCLUDED.server_version, detected_at = now()",
            location,
            &unique
        )
        .execute(self.pool())
        .await?
        .rows_affected();
        if recorded != unique.len() as u64 {
            return Err(invalid(
                "Conflict report names a lesson unknown to this project",
            ));
        }
        Ok(recorded)
    }

    pub async fn knowledge_project_health(&self, project: &str) -> Result<Value, PersistError> {
        let identity = self.resolve_knowledge_project(project).await?;
        Ok(sqlx::query_file_scalar!(
            "src/store/persistence/project_knowledge/health.sql",
            identity.id
        )
        .fetch_one(self.pool())
        .await?)
    }
}
