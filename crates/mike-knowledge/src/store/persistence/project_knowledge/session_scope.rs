use crate::lesson_cache::ChangeMark;
use crate::store::persistence::PersistError;
use std::collections::HashMap;
use uuid::Uuid;

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub async fn lesson_session_projects_fingerprint(self) -> Result<ChangeMark, PersistError> {
        crate::metrics::time_db("lesson_session_projects_fingerprint", async move {
            let fingerprint = sqlx::query_as!(
                ChangeMark,
                r#"SELECT generation AS "generation!", 0::bigint AS "expired!"
                     FROM memory.change_generations
                    WHERE name = 'session_projects'"#
            )
            .fetch_one(self.pool())
            .await?;
            Ok(fingerprint)
        })
        .await
    }

    pub async fn lesson_session_projects(self) -> Result<HashMap<String, String>, PersistError> {
        let rows = sqlx::query!(
            r#"SELECT s.session_uuid::text AS "session_uuid!", p.project_path FROM sessions.sessions s
             JOIN projects.projects p ON p.id = s.project_id WHERE s.location_uuid IS NOT NULL OR s.project_resolution = 'operator'"#,
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| (row.session_uuid, row.project_path))
            .collect())
    }

    pub async fn resolve_lesson_project(
        self,
        session: &str,
        path: &str,
    ) -> Result<i64, PersistError> {
        if let Ok(uuid) = session.parse::<Uuid>() {
            let project = sqlx::query_scalar!(
                "SELECT project_id FROM sessions.sessions WHERE session_uuid = $1 AND (location_uuid IS NOT NULL OR project_resolution = 'operator')",
                uuid,
            )
            .fetch_optional(self.pool())
            .await?;
            if let Some(project) = project {
                return Ok(project);
            }
        }
        if let Some(project) = self.find_knowledge_project(path).await? {
            return Ok(project.id);
        }
        self.upsert_project_returning_id(path).await
    }
}
