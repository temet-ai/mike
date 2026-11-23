use crate::store::persistence::PersistError;

impl crate::store::persistence::lessons_repo::LessonsDb<'_> {
    pub async fn retire_lesson_consolidated(
        self,
        lesson_id: i64,
        reason: &str,
        retired_by_lesson_id: Option<i64>,
    ) -> Result<bool, PersistError> {
        Ok(sqlx::query!(
            "UPDATE memory.lessons
                SET retired_at = now(), retired_kind = 'consolidated', retired_reason = $2,
                    retired_by_lesson_id = $3
              WHERE id = $1 AND retired_at IS NULL AND archived_at IS NULL
                AND $3::bigint IS DISTINCT FROM id",
            lesson_id,
            reason,
            retired_by_lesson_id
        )
        .execute(&self.pool)
        .await?
        .rows_affected()
            > 0)
    }

    pub async fn restore_consolidated_lesson(self, lesson_id: i64) -> Result<bool, PersistError> {
        Ok(sqlx::query!(
            "UPDATE memory.lessons
                SET retired_at = NULL, retired_kind = NULL, retired_reason = NULL,
                    retired_by_lesson_id = NULL
              WHERE id = $1 AND retired_kind = 'consolidated'",
            lesson_id
        )
        .execute(&self.pool)
        .await?
        .rows_affected()
            > 0)
    }

    pub async fn update_lesson_text(
        self,
        lesson_id: i64,
        text: &str,
    ) -> Result<Option<i32>, PersistError> {
        Ok(sqlx::query_scalar!(
            "UPDATE memory.lessons SET text = $2
              WHERE id = $1 AND archived_at IS NULL AND btrim($2) <> ''
             RETURNING revision",
            lesson_id,
            text
        )
        .fetch_optional(&self.pool)
        .await?)
    }
}
