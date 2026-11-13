use crate::store::persistence::PersistError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UntopicedLesson {
    pub id: i64,
    pub project_id: i64,
    pub text: String,
}

impl crate::store::persistence::lessons_repo::LessonsDb<'_> {
    pub async fn untopiced_lesson_batch(
        self,
        limit: i64,
    ) -> Result<Vec<UntopicedLesson>, PersistError> {
        crate::metrics::time_db("untopiced_lesson_batch", async move {
            let rows = sqlx::query_as!(
                UntopicedLesson,
                r#"WITH busiest AS (
                       SELECT l.project_id
                         FROM memory.lessons l
                         JOIN memory.lesson_stages ls
                           ON ls.id = l.stage_id AND ls.code IN ('daily', 'long_term')
                        WHERE l.archived_at IS NULL
                          AND l.topic IS NULL
                          AND l.project_id IS NOT NULL
                        GROUP BY l.project_id
                        ORDER BY count(*) DESC, l.project_id
                        LIMIT 1)
                   SELECT l.id, l.project_id AS "project_id!", l.text
                     FROM memory.lessons l
                     JOIN busiest b ON b.project_id = l.project_id
                     JOIN memory.lesson_stages ls
                       ON ls.id = l.stage_id AND ls.code IN ('daily', 'long_term')
                    WHERE l.archived_at IS NULL
                      AND l.topic IS NULL
                    ORDER BY l.id
                    LIMIT $1"#,
                limit
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn project_topics(self, project_id: i64) -> Result<Vec<String>, PersistError> {
        crate::metrics::time_db("project_topics", async move {
            let rows = sqlx::query!(
                r#"SELECT DISTINCT l.topic AS "topic!"
                     FROM memory.lessons l
                    WHERE l.project_id = $1
                      AND l.topic IS NOT NULL
                      AND l.archived_at IS NULL
                      AND l.topic NOT LIKE 'shared:%'
                      AND l.topic NOT LIKE 'verifier:%'
                    ORDER BY 1"#,
                project_id
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows.into_iter().map(|r| r.topic).collect())
        })
        .await
    }

    pub async fn set_lesson_topics(
        self,
        ids: &[i64],
        topics: &[String],
    ) -> Result<u64, PersistError> {
        crate::metrics::time_db("set_lesson_topics", async move {
            let result = sqlx::query!(
                "UPDATE memory.lessons l
                    SET topic = v.topic
                   FROM unnest($1::bigint[], $2::text[]) AS v(id, topic)
                  WHERE l.id = v.id
                    AND l.topic IS NULL",
                ids,
                topics
            )
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected())
        })
        .await
    }
}
