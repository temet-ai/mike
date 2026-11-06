use super::{PersistError, PersistHandle};

#[derive(Debug, Clone, Default)]
pub struct StartupRowCounts {
    pub requests: i64,
    pub sessions: i64,
    pub tool_uses: i64,
    pub tool_results: i64,
    pub chunks: i64,
}

pub(crate) async fn schema_exists(pool: &sqlx::PgPool) -> Result<bool, PersistError> {
    let exists = sqlx::query_scalar!(
        "SELECT EXISTS (SELECT 1 FROM information_schema.schemata WHERE schema_name = 'mike')",
    )
    .fetch_one(pool)
    .await?;
    Ok(exists.unwrap_or(false))
}

impl PersistHandle {
    pub(crate) async fn schema_exists(&self) -> Result<bool, PersistError> {
        schema_exists(&self.pool).await
    }
    pub(crate) async fn sweep_expired_sessions(
        &self,
        default_ttl: std::time::Duration,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<u64, PersistError> {
        crate::metrics::time_db("sweep_expired_sessions", async move {
            let default_ttl_pg = sqlx::postgres::types::PgInterval {
                months: 0,
                days: 0,
                microseconds: default_ttl.as_micros() as i64,
            };
            let r1 = sqlx::query!(
                "DELETE FROM sessions.sessions s
                 USING (
                     SELECT s2.id
                     FROM sessions.sessions s2
                     LEFT JOIN projects.projects p ON p.id = s2.project_id
                     LEFT JOIN (
                         SELECT DISTINCT r.session_id
                         FROM requests.requests r
                         JOIN requests.statuses rs ON rs.id = r.status_id
                                                  AND rs.code IN ('pending', 'streaming')
                         WHERE r.created_at >= $2::timestamptz - $1::interval
                     ) inflight ON inflight.session_id = s2.id
                     WHERE NOT s2.pinned
                       AND s2.ttl_override IS NULL
                       AND COALESCE(p.pinned, FALSE) = FALSE
                       AND p.ttl_override IS NULL
                       AND s2.last_seen < $2::timestamptz - $1::interval
                       AND inflight.session_id IS NULL
                 ) sub
                 WHERE s.id = sub.id",
                &default_ttl_pg,
                now,
            )
            .execute(&self.pool)
            .await?;

            let r2 = sqlx::query!(
                "DELETE FROM sessions.sessions s
                 USING (
                     SELECT
                         s2.id,
                         COALESCE(s2.ttl_override, p.ttl_override, $1::interval) AS effective_ttl,
                         COALESCE(p.pinned, FALSE) AS project_pinned
                     FROM sessions.sessions s2
                     LEFT JOIN projects.projects p ON p.id = s2.project_id
                     LEFT JOIN (
                         SELECT DISTINCT r.session_id
                         FROM requests.requests r
                         JOIN requests.statuses rs ON rs.id = r.status_id
                                                  AND rs.code IN ('pending', 'streaming')
                         WHERE r.created_at >= $2::timestamptz - $1::interval
                     ) inflight ON inflight.session_id = s2.id
                     WHERE NOT s2.pinned
                       AND (s2.ttl_override IS NOT NULL OR p.ttl_override IS NOT NULL)
                       AND inflight.session_id IS NULL
                 ) sub
                 WHERE s.id = sub.id
                   AND NOT sub.project_pinned
                   AND s.last_seen < $2::timestamptz - sub.effective_ttl",
                &default_ttl_pg,
                now,
            )
            .execute(&self.pool)
            .await?;

            let removed = r1.rows_affected() + r2.rows_affected();
            if removed > 0 {
                self.forget_cached_sessions();
            }
            Ok(removed)
        })
        .await
    }
    pub async fn startup_row_counts(&self) -> Result<StartupRowCounts, PersistError> {
        crate::metrics::time_db("startup_row_counts", async move {
            let requests =
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*)::BIGINT FROM requests.requests")
                    .fetch_one(&self.pool)
                    .await
                    .unwrap_or(0);
            let sessions =
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*)::BIGINT FROM sessions.sessions")
                    .fetch_one(&self.pool)
                    .await
                    .unwrap_or(0);
            let tool_uses = sqlx::query_scalar::<_, i64>("SELECT COUNT(*)::BIGINT FROM tools.uses")
                .fetch_one(&self.pool)
                .await
                .unwrap_or(0);
            let tool_results =
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*)::BIGINT FROM tools.results")
                    .fetch_one(&self.pool)
                    .await
                    .unwrap_or(0);
            let chunks = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*)::BIGINT FROM memory.operator_turn_chunks",
            )
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);
            Ok(StartupRowCounts {
                requests,
                sessions,
                tool_uses,
                tool_results,
                chunks,
            })
        })
        .await
    }
}
