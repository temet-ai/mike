use uuid::Uuid;

use super::super::PersistError;

pub struct SessionInjectedLessonRow {
    pub lesson_id: i64,
    pub lesson_text: String,
    pub kind: String,
    pub injection_count: i64,
    pub first_injected_at: chrono::DateTime<chrono::Utc>,
    pub last_injected_at: chrono::DateTime<chrono::Utc>,
    pub origins: Vec<String>,
}

pub struct SessionRecordedLessonRow {
    pub lesson_id: i64,
    pub lesson_text: String,
    pub kind: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct SessionLessonsRows {
    pub injected: Vec<SessionInjectedLessonRow>,
    pub recorded: Vec<SessionRecordedLessonRow>,
}

impl crate::store::persistence::lessons_repo::LessonsDb<'_> {
    pub async fn session_lessons(
        self,
        session_uuid: Uuid,
    ) -> Result<Option<SessionLessonsRows>, PersistError> {
        crate::metrics::time_db("session_lessons", async move {
            let Some(session_id) = self.session_row_id(session_uuid).await? else {
                return Ok(None);
            };

            let injected = sqlx::query_as!(
                SessionInjectedLessonRow,
                r#"SELECT l.id AS "lesson_id!",
                        l.text AS "lesson_text!",
                        lk.code AS "kind!",
                        count(*) AS "injection_count!",
                        min(li.injected_at) AS "first_injected_at!",
                        max(li.injected_at) AS "last_injected_at!",
                        array_agg(DISTINCT li.origin ORDER BY li.origin) AS "origins!"
                   FROM memory.lesson_injections li
                   JOIN memory.lessons l ON l.id = li.lesson_id
                   JOIN memory.lesson_kinds lk ON lk.id = l.kind_id
                  WHERE li.session_id = $1
                  GROUP BY l.id, l.text, lk.code
                  ORDER BY l.id"#,
                session_id,
            )
            .fetch_all(&self.pool)
            .await?;

            let recorded = sqlx::query_as!(
                SessionRecordedLessonRow,
                r#"SELECT l.id AS "lesson_id!",
                        l.text AS "lesson_text!",
                        lk.code AS "kind!",
                        l.created_at AS "created_at!"
                   FROM memory.lessons l
                   JOIN memory.lesson_kinds lk ON lk.id = l.kind_id
                  WHERE l.session_id = $1
                  ORDER BY l.id"#,
                session_id,
            )
            .fetch_all(&self.pool)
            .await?;

            Ok(Some(SessionLessonsRows { injected, recorded }))
        })
        .await
    }
}
