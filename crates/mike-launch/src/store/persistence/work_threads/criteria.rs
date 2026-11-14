use crate::store::persistence::PersistError;

pub const MAX_LINKED_THREADS: i64 = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadCriteria {
    pub thread_slug: String,
    pub title: String,
    pub criteria_text: String,
}

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn fetch_linked_thread_criteria(
        self,
        session_db_id: i64,
    ) -> Result<Vec<ThreadCriteria>, PersistError> {
        crate::metrics::time_db("fetch_linked_thread_criteria", async move {
            let rows = sqlx::query!(
                r#"SELECT t.slug, COALESCE(t.title, '') AS "title!", c.criteria_text AS "criteria_text!"
                     FROM sessions.sessions s
                     JOIN work.thread_sessions ts ON ts.session_uuid = s.session_uuid
                     JOIN work.threads t ON t.id = ts.thread_id
                     JOIN work.thread_acceptance_criteria c ON c.thread_id = t.id
                    WHERE s.id = $1
                      AND btrim(COALESCE(c.criteria_text, '')) <> ''
                    ORDER BY ts.last_seen_at DESC, t.id DESC
                    LIMIT $2"#,
                session_db_id,
                MAX_LINKED_THREADS,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|r| ThreadCriteria {
                    thread_slug: r.slug,
                    title: r.title,
                    criteria_text: r.criteria_text,
                })
                .collect())
        })
        .await
    }
}
