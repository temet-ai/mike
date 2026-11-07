use serde_json::Value;
use uuid::Uuid;

use super::SourceInput;
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

    pub async fn record_knowledge_roots_missing(
        &self,
        roots: &[SourceInput],
    ) -> Result<u64, PersistError> {
        if roots.is_empty() {
            return Ok(0);
        }
        let locations: Vec<Uuid> = roots.iter().map(|root| root.location_uuid).collect();
        let names: Vec<String> = roots.iter().map(|root| root.name.clone()).collect();
        let paths: Vec<String> = roots.iter().map(|root| root.root_path.clone()).collect();
        Ok(sqlx::query!(
            "WITH missing AS (
                 SELECT x.location_uuid, x.name,
                        'the source root ' || x.root_path || ' does not exist; the earlier index is kept and may be out of date' AS error
                   FROM unnest($1::uuid[], $2::text[], $3::text[]) AS x(location_uuid, name, root_path)
             )
             UPDATE memory.knowledge_sources s
                SET last_failure = m.error, last_failure_at = now(),
                    last_failure_generation = s.generation, last_heartbeat_at = now()
               FROM missing m
              WHERE s.location_uuid = m.location_uuid AND s.name = m.name
                AND (s.last_failure IS DISTINCT FROM m.error
                     OR s.last_failure_generation IS DISTINCT FROM s.generation)",
            &locations,
            &names,
            &paths
        )
        .execute(self.pool())
        .await?
        .rows_affected())
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
