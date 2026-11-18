use serde::Serialize;
use uuid::Uuid;

use crate::store::persistence::PersistError;

const TWIN_LIMIT: i64 = 5;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectTwin {
    pub project_uuid: Uuid,
    pub name: String,
    pub project_path: String,
    pub items: i64,
}

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub async fn knowledge_twins(self, project_id: i64) -> Result<Vec<ProjectTwin>, PersistError> {
        Ok(sqlx::query_as!(
            ProjectTwin,
            r#"WITH me AS (
                   SELECT lower(regexp_replace(p.project_path, '^.*/', '')) AS folder
                     FROM projects.projects p WHERE p.id = $1
               ), origins AS (
                   SELECT DISTINCT l.origin_url FROM projects.locations l
                    WHERE l.project_id = $1 AND l.origin_url IS NOT NULL
               )
               SELECT p.project_uuid AS "project_uuid!",
                      COALESCE(p.custom_name, p.display_name, p.project_path) AS "name!",
                      p.project_path AS "project_path!", c.items AS "items!"
                 FROM projects.projects p
                 CROSS JOIN LATERAL (
                      SELECT count(*) AS items FROM memory.knowledge_items i
                       WHERE i.project_id = p.id) c
                WHERE p.merged_into_id IS NULL AND p.id <> $1 AND c.items > 0
                  AND (EXISTS (SELECT 1 FROM projects.locations l
                                JOIN origins o ON o.origin_url = l.origin_url
                               WHERE l.project_id = p.id)
                       OR (SELECT folder FROM me) <> ''
                          AND lower(regexp_replace(p.project_path, '^.*/', ''))
                              = (SELECT folder FROM me))
                ORDER BY c.items DESC, p.id
                LIMIT $2"#,
            project_id,
            TWIN_LIMIT,
        )
        .fetch_all(self.pool())
        .await?)
    }
}
