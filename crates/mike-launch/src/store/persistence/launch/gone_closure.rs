use chrono::{DateTime, Utc};

use crate::store::persistence::PersistError;

const CLOSE_GONE_LAUNCHES: &str = "
WITH due AS (
    SELECT l.id,
           CASE WHEN r.reaped_at IS NOT NULL THEN 'process reaped'
                WHEN r.released_at IS NOT NULL THEN 'session ended'
                WHEN l.opened AND act.observed_at IS NULL THEN 'session never reached mike'
                WHEN l.opened THEN 'session went quiet'
                ELSE 'never started'
           END AS reason,
           CASE WHEN r.released_at IS NOT NULL OR NOT l.opened THEN TRUE END AS exited
      FROM launch.launches l
      LEFT JOIN sessions.sessions s ON s.session_uuid = l.session_uuid
      LEFT JOIN workspace.session_registrations r ON r.session_id = s.id
      LEFT JOIN LATERAL runtime.session_activity(s.id) act ON TRUE
     WHERE l.closed_at IS NULL
       AND (r.released_at >= l.created_at
            OR (l.created_at < $1
                AND (r.session_id IS NULL OR r.released_at IS NOT NULL)
                AND NOT COALESCE(act.observed_at >= $1, FALSE)))
)
UPDATE launch.launches l
   SET closed_at            = now(),
       close_reason         = due.reason,
       close_process_exited = due.exited
  FROM due
 WHERE l.id = due.id
   AND l.closed_at IS NULL
";

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn close_gone_launches(
        self,
        quiet_since: DateTime<Utc>,
    ) -> Result<u64, PersistError> {
        crate::metrics::time_db("launch_close_gone_launches", async move {
            let done = sqlx::query(CLOSE_GONE_LAUNCHES) // grader-allow: UPDATE across launch, sessions and workspace schemas with no .sqlx entry
                .bind(quiet_since)
                .execute(&self.pool)
                .await?;
            Ok(done.rows_affected())
        })
        .await
    }
}
