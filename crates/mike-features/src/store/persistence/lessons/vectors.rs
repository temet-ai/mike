use super::ChangeMark;
use crate::store::persistence::PersistError;

impl crate::store::persistence::lessons_repo::LessonsDb<'_> {
    pub async fn injectable_lesson_vectors_fingerprint(self) -> Result<ChangeMark, PersistError> {
        crate::metrics::time_db("injectable_lesson_vectors_fingerprint", async move {
            let fingerprint = sqlx::query_as!(
                ChangeMark,
                r#"SELECT generation AS "generation!", 0::bigint AS "expired!"
                     FROM memory.change_generations
                    WHERE name = 'lessons'"#
            )
            .fetch_one(self.pool())
            .await?;
            Ok(fingerprint)
        })
        .await
    }

    pub async fn injectable_lesson_vectors(
        self,
        model: &str,
    ) -> Result<Vec<(i64, Vec<f32>)>, PersistError> {
        let model = model.to_owned();
        crate::metrics::time_db("injectable_lesson_vectors", async move {
            let rows = sqlx::query!(
                r#"SELECT id, embedding::real[] AS "embedding!: Vec<f32>"
                     FROM memory.lessons
                    WHERE embedding_model = $1 AND embedding IS NOT NULL AND archived_at IS NULL"#,
                model
            )
            .fetch_all(self.pool())
            .await?;
            Ok(rows
                .into_iter()
                .map(|row| (row.id, row.embedding))
                .collect())
        })
        .await
    }
}
