use super::{ReapCandidate, ReapOutcome};
use crate::store::persistence::{PersistError, PersistHandle};

impl PersistHandle {
    pub async fn fetch_reap_candidates(
        &self,
        current_boot_id: &str,
    ) -> Result<Vec<ReapCandidate>, PersistError> {
        crate::metrics::time_db("workspace_fetch_reap_candidates", async move {
            let rows = sqlx::query!(
                r#"SELECT session_id, pid AS "pid!"
                     FROM workspace.session_registrations
                    WHERE released_at IS NULL
                      AND pid IS NOT NULL
                      AND boot_id = $1"#,
                current_boot_id,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|r| ReapCandidate {
                    session_id: r.session_id,
                    pid: r.pid,
                })
                .collect())
        })
        .await
    }

    pub async fn fetch_lapsed_registration_ids(
        &self,
        lapsed_before: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<i64>, PersistError> {
        crate::metrics::time_db("workspace_fetch_lapsed_registrations", async move {
            let ids = sqlx::query_scalar!(
                r#"SELECT session_id
                     FROM workspace.session_registrations
                    WHERE released_at IS NULL
                      AND lease_expires_at < $1"#,
                lapsed_before,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(ids)
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
            Ok(ReapOutcome { reaped: row.count })
        })
        .await
    }
}
