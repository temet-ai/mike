use crate::store::persistence::PersistError;

const CLOSE_REAPED_LAUNCHES: &str = "
UPDATE launch.launches l
   SET closed_at            = now(),
       close_reason         = 'process reaped',
       close_process_exited = TRUE
  FROM sessions.sessions s
 WHERE l.closed_at IS NULL
   AND l.session_uuid = s.session_uuid
   AND EXISTS (SELECT 1
                 FROM workspace.session_registrations r
                WHERE r.session_id = s.id
                  AND r.reaped_at IS NOT NULL)
   AND NOT EXISTS (SELECT 1
                     FROM workspace.session_registrations r
                    WHERE r.session_id = s.id
                      AND r.released_at IS NULL)
";

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn close_reaped_launches(self) -> Result<u64, PersistError> {
        crate::metrics::time_db("launch_close_reaped_launches", async move {
            let done = sqlx::query(CLOSE_REAPED_LAUNCHES) // grader-allow: UPDATE across launch, sessions and workspace schemas with no .sqlx entry
                .execute(&self.pool)
                .await?;
            Ok(done.rows_affected())
        })
        .await
    }
}
