use crate::store::persistence::PersistError;
use crate::store::persistence::operator_turns::vector_to_text_literal;

impl crate::store::persistence::lessons_repo::LessonsDb<'_> {
    pub async fn pending_lesson_vector_repairs(
        self,
        model: &str,
        after_id: i64,
        limit: i64,
    ) -> Result<Vec<(i64, String)>, PersistError> {
        let rows = sqlx::query!(
            "SELECT l.id, l.text FROM memory.lessons l
             JOIN memory.lesson_stages stage ON stage.id = l.stage_id
             WHERE l.id > $2 AND l.archived_at IS NULL
               AND (l.embedding IS NULL OR l.embedding_model IS DISTINCT FROM $1)
               AND (l.expires_at IS NULL OR l.expires_at > NOW())
               AND stage.code IN ('daily', 'long_term')
               AND NOT EXISTS (
                   SELECT 1 FROM memory.lesson_relations r
                   JOIN memory.lesson_relation_kinds k ON k.id = r.kind_id AND k.code = 'supersedes'
                   JOIN memory.lesson_relation_statuses s ON s.id = r.status_id AND s.code = 'confirmed'
                   WHERE r.to_lesson_id = l.id
               )
             ORDER BY l.id LIMIT $3",
            model,
            after_id,
            limit,
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|row| (row.id, row.text)).collect())
    }

    pub async fn write_repaired_lesson_vector(
        self,
        id: i64,
        expected_text: &str,
        vector: &[f32],
        model: &str,
    ) -> Result<bool, PersistError> {
        let literal = vector_to_text_literal(vector);
        let result = sqlx::query!(
            "UPDATE memory.lessons l
             SET embedding = $3::text::vector, embedding_model = $4, embedded_at = NOW()
             FROM memory.lesson_stages stage
             WHERE l.id = $1 AND l.text = $2 AND l.archived_at IS NULL
               AND stage.id = l.stage_id AND stage.code IN ('daily', 'long_term')
               AND (embedding IS NULL OR embedding_model IS DISTINCT FROM $4)",
            id,
            expected_text,
            literal,
            model,
        )
        .execute(self.pool())
        .await?;
        Ok(result.rows_affected() == 1)
    }
}
