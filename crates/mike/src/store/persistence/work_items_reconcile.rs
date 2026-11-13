use chrono::{DateTime, Utc};

use super::PersistError;
use crate::work_items::output_end::Ended;

pub(crate) struct OpenJob {
    pub id: i64,
    pub output_path: String,
}

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub(crate) async fn open_background_jobs(
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

    pub(crate) async fn close_job_from_its_output(
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
}

#[cfg(test)]
#[path = "work_items_reconcile_tests.rs"]
mod tests;
