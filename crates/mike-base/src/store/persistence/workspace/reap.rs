use super::{ReapCandidate, ReapOutcome};
use crate::store::persistence::{PersistError, PersistHandle};

impl PersistHandle {
    pub async fn fetch_reap_candidates(
        &self,
        current_boot_id: &str,
        lapsed_before: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<ReapCandidate>, PersistError> {
        crate::metrics::time_db("workspace_fetch_reap_candidates", async move {
            let rows = sqlx::query!(
                r#"SELECT session_id,
                          CASE WHEN boot_id = $1 THEN pid END AS pid,
                          lease_expires_at < $2 AS "lapsed!"
                     FROM workspace.session_registrations
                    WHERE released_at IS NULL
                      AND ((pid IS NOT NULL AND boot_id = $1) OR lease_expires_at < $2)"#,
                current_boot_id,
                lapsed_before,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|r| ReapCandidate {
                    session_id: r.session_id,
                    pid: r.pid,
                    lapsed: r.lapsed,
                })
                .collect())
        })
        .await
    }

    pub async fn reap_dead_registrations(
        &self,
        session_ids: &[i64],
    ) -> Result<ReapOutcome, PersistError> {
        if session_ids.is_empty() {
            return Ok(ReapOutcome { reaped: 0 });
        }
        crate::metrics::time_db("workspace_reap_dead_registrations", async move {
            let row = sqlx::query!(
                r#"WITH reaped AS (
                       UPDATE workspace.session_registrations r
                          SET released_at = now(),
                              reaped_at   = now()
                        WHERE r.session_id = ANY($1)
                          AND r.released_at IS NULL
                    RETURNING r.session_id, r.project_id
                   ),
                   ledger AS (
                       INSERT INTO workspace.coordination_events
                           (project_id, session_id, actor_label, kind_id, subject, detail)
                       SELECT reaped.project_id, reaped.session_id, 'workspace-reaper',
                              ek.id, s.session_uuid::text, '{}'::jsonb
                         FROM reaped
                         JOIN sessions.sessions s ON s.id = reaped.session_id
                         CROSS JOIN workspace.event_kinds ek
                        WHERE ek.code = 'reaped'
                       RETURNING 1
                   )
                   SELECT count(*) AS "count!" FROM reaped"#,
                session_ids,
            )
            .fetch_one(&self.pool)
            .await?;
            if row.count > 0 {
                self.note_registration_released();
            }
            Ok(ReapOutcome { reaped: row.count })
        })
        .await
    }
}
