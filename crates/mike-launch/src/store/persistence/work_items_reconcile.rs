use chrono::{DateTime, Utc};

use super::PersistError;
use crate::work_items::output_end::Ended;

pub struct OpenJob {
    pub id: i64,
    pub output_path: String,
}

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn open_background_jobs(
        self,
        session_db_id: i64,
    ) -> Result<Vec<OpenJob>, PersistError> {
        crate::metrics::time_db("work_items_open_jobs", async move {
            let rows = sqlx::query_as!(
                OpenJob,
                r#"SELECT id, output_path AS "output_path!"
                     FROM agents.work_items
                    WHERE session_id = $1
                      AND kind = 'bash'
                      AND state = 'running'
                      AND output_path IS NOT NULL"#,
                session_db_id,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn close_job_from_its_output(
        self,
        id: i64,
        ended: Ended,
        finished_at: DateTime<Utc>,
    ) -> Result<u64, PersistError> {
        crate::metrics::time_db("work_items_close_from_output", async move {
            let done = sqlx::query!(
                r#"UPDATE agents.work_items
                      SET state = $2,
                          finished_at = $3,
                          exit_code = $4,
                          summary = COALESCE(summary, 'read from the last line of the job''s output file')
                    WHERE id = $1
                      AND state = 'running'"#,
                id,
                ended.state,
                finished_at,
                ended.exit_code,
            )
            .execute(&self.pool)
            .await?;
            Ok(done.rows_affected())
        })
        .await
    }

    pub async fn sessions_with_abandoned_items(
        self,
        quiet_hours: i32,
    ) -> Result<Vec<i64>, PersistError> {
        crate::metrics::time_db("work_items_abandoned_sessions", async move {
            let ids = sqlx::query_scalar!(
                r#"SELECT DISTINCT w.session_id AS "session_id!"
                     FROM agents.work_items w
                     JOIN sessions.sessions s ON s.id = w.session_id
                    WHERE w.state = 'running'
                      AND s.last_seen < now() - make_interval(hours => $1::int)"#,
                quiet_hours,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(ids)
        })
        .await
    }

    pub async fn close_abandoned_items(self, quiet_hours: i32) -> Result<u64, PersistError> {
        crate::metrics::time_db(
            "work_items_close_abandoned",
            crate::store::persistence::work_items::close_abandoned_items(&self.pool, quiet_hours),
        )
        .await
    }
}

#[cfg(test)]
#[path = "work_items_reconcile_tests.rs"]
mod tests;
