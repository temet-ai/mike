use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChangeMark {
    pub generation: i64,
    pub expired: i64,
}

#[derive(Debug, Clone)]
pub struct InjectableLessonRow {
    pub id: i64,
    pub project_path: String,
    pub kind: String,
    pub text: String,
    pub topic: Option<String>,
    pub agent_id: Option<i64>,
}

impl PersistHandle {
    pub(crate) async fn injectable_lessons_fingerprint(&self) -> Result<ChangeMark, PersistError> {
        crate::metrics::time_db("injectable_lessons_fingerprint", async move {
            let fingerprint = sqlx::query_as!(
                ChangeMark,
                r#"SELECT g.generation AS "generation!",
                          (SELECT count(*) FROM memory.lessons l
                            WHERE l.expires_at IS NOT NULL AND l.expires_at <= NOW()) AS "expired!"
                     FROM memory.change_generations g
                    WHERE g.name = 'lessons'"#
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(fingerprint)
        })
        .await
    }

    pub(crate) async fn list_project_injectable_lessons(
        &self,
    ) -> Result<Vec<InjectableLessonRow>, PersistError> {
        crate::metrics::time_db("list_project_injectable_lessons", async move {
            let rows = sqlx::query_as!(
                InjectableLessonRow,
                r#"SELECT l.id AS "id!", p.project_path AS "project_path!", lk.code AS "kind!",
                          l.text AS "text!", l.topic, l.agent_id
                     FROM memory.lessons l
                     JOIN projects.projects p ON p.id = l.project_id
                     JOIN memory.lesson_kinds lk ON lk.id = l.kind_id
                     JOIN memory.lesson_stages ls ON ls.id = l.stage_id
                     JOIN memory.lesson_sources src ON src.id = l.source_id
                    WHERE (ls.code IN ('daily', 'long_term') OR (ls.code = 'raw' AND src.code = 'operator'))
                      AND (l.expires_at IS NULL OR l.expires_at > NOW())
                      AND NOT EXISTS (
                          SELECT 1
                            FROM memory.lesson_relations r
                            JOIN memory.lesson_relation_kinds rk ON rk.id = r.kind_id AND rk.code = 'supersedes'
                            JOIN memory.lesson_relation_statuses rs ON rs.id = r.status_id AND rs.code = 'confirmed'
                           WHERE r.to_lesson_id = l.id
                      )
                      AND (ls.code <> 'raw' OR NOT EXISTS (
                          SELECT 1 FROM memory.lesson_shape_findings f
                           WHERE f.lesson_id = l.id AND f.resolution IS NULL
                      ))
                    ORDER BY l.created_at DESC"#
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}
