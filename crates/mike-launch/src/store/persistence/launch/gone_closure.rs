use chrono::{DateTime, Utc};

use crate::store::persistence::PersistError;

const CLOSE_GONE_LAUNCHES: &str = "
WITH open_launches AS (
    SELECT l.id, l.created_at, l.opened,
           r.session_id AS registered, r.released_at, r.reaped_at, act.observed_at
      FROM launch.launches l
      LEFT JOIN sessions.sessions s ON s.session_uuid = l.session_uuid
      LEFT JOIN workspace.session_registrations r ON r.session_id = s.id
      LEFT JOIN LATERAL runtime.session_activity(s.id) act ON TRUE
     WHERE l.closed_at IS NULL
),
due AS (
    SELECT o.id,
           CASE WHEN o.reaped_at IS NOT NULL THEN 'process reaped'
                WHEN o.released_at IS NOT NULL THEN 'session ended'
                WHEN o.opened AND o.observed_at IS NULL THEN 'session never reached mike'
                WHEN o.opened THEN 'session went quiet'
                ELSE 'never started'
           END AS reason,
           CASE WHEN o.released_at IS NOT NULL OR NOT o.opened THEN TRUE END AS exited
      FROM open_launches o
     WHERE o.released_at >= o.created_at
        OR (o.created_at < $1
            AND (o.registered IS NULL OR o.released_at IS NOT NULL)
            AND NOT COALESCE(o.observed_at >= $1, FALSE))
),
closed AS (
    UPDATE launch.launches l
       SET closed_at            = now(),
           close_reason         = due.reason,
           close_process_exited = due.exited
      FROM due
     WHERE l.id = due.id
       AND l.closed_at IS NULL
    RETURNING l.id
)
SELECT (SELECT count(*) FROM closed) AS closed,
       (SELECT min(GREATEST(o.created_at, o.observed_at) + (now() - $1))
          FROM open_launches o
         WHERE o.id NOT IN (SELECT id FROM due)
           AND (o.registered IS NULL OR o.released_at IS NOT NULL)) AS next_due
";

/// What one gone-closure sweep did, and when it next has a launch that time
/// alone can close. `next_due` is None when every open launch is held by a live
/// registration, which only a release (or a launch change) can close.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GoneSweep {
    pub closed: u64,
    pub next_due: Option<DateTime<Utc>>,
}

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn close_gone_launches(
        self,
        quiet_since: DateTime<Utc>,
    ) -> Result<GoneSweep, PersistError> {
        crate::metrics::time_db("launch_close_gone_launches", async move {
            let (closed, next_due): (i64, Option<DateTime<Utc>>) =
                sqlx::query_as(CLOSE_GONE_LAUNCHES) // grader-allow: UPDATE across launch, sessions and workspace schemas with no .sqlx entry
                    .bind(quiet_since)
                    .fetch_one(&self.pool)
                    .await?;
            Ok(GoneSweep {
                closed: u64::try_from(closed).unwrap_or(0),
                next_due,
            })
        })
        .await
    }
}
