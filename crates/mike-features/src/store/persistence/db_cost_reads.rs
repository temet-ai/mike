use std::collections::HashMap;

use chrono::{DateTime, Utc};

use super::PersistError;

#[derive(Debug, Clone, PartialEq)]
pub struct StatementCounters {
    pub queryid: i64,
    pub calls: i64,
    pub generic_plan_calls: i64,
    pub custom_plan_calls: i64,
    pub exec_ms: f64,
    pub blks_hit: i64,
    pub blks_read: i64,
    pub blks_dirtied: i64,
    pub blks_written: i64,
}

#[derive(Debug, Clone)]
pub struct StatementStatsReading {
    pub taken_at: DateTime<Utc>,
    pub stats_reset: Option<DateTime<Utc>>,
    pub statements: Vec<StatementCounters>,
}

impl crate::store::persistence::state_repo::StateDb<'_> {
    pub async fn read_statement_stats(self) -> Result<Option<StatementStatsReading>, PersistError> {
        crate::metrics::time_db("db_cost_read_snapshot", async move {
            let present = sqlx::query_scalar!(
                "SELECT to_regclass('pg_stat_statements_info')::text"
            )
            .fetch_one(&self.pool)
            .await?;
            if present.is_none() {
                return Ok(None);
            }
            let clock = sqlx::query!(
                r#"SELECT i.stats_reset, clock_timestamp() AS "taken_at!"
                   FROM pg_stat_statements_info i"#
            )
            .fetch_one(&self.pool)
            .await?;
            let statements = sqlx::query_as!(
                StatementCounters,
                r#"SELECT s.queryid AS "queryid!",
                          s.calls AS "calls!",
                          s.generic_plan_calls AS "generic_plan_calls!",
                          s.custom_plan_calls AS "custom_plan_calls!",
                          s.total_exec_time AS "exec_ms!",
                          s.shared_blks_hit AS "blks_hit!",
                          s.shared_blks_read AS "blks_read!",
                          s.shared_blks_dirtied AS "blks_dirtied!",
                          s.shared_blks_written AS "blks_written!"
                   FROM pg_stat_statements(true) s
                   WHERE s.toplevel
                     AND s.queryid IS NOT NULL
                     AND s.query !~* '^\s*(COPY|EXPLAIN|VACUUM|ANALYZE|CREATE|ALTER|DROP|REINDEX|CLUSTER|TRUNCATE|REFRESH|SET|SHOW|RESET|BEGIN|COMMIT|ROLLBACK|START|SAVEPOINT|RELEASE|PREPARE|DEALLOCATE|LISTEN|UNLISTEN|GRANT|REVOKE|COMMENT|LOCK|DISCARD|CHECKPOINT)\M'
                     AND s.userid = (SELECT r.oid FROM pg_catalog.pg_roles r WHERE r.rolname = current_user)
                     AND s.dbid = (SELECT d.oid FROM pg_catalog.pg_database d WHERE d.datname = current_database())"#
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(Some(StatementStatsReading {
                taken_at: clock.taken_at,
                stats_reset: clock.stats_reset,
                statements,
            }))
        })
        .await
    }

    pub async fn read_statement_texts(
        self,
        queryids: &[i64],
    ) -> Result<HashMap<i64, String>, PersistError> {
        if queryids.is_empty() {
            return Ok(HashMap::new());
        }
        crate::metrics::time_db("db_cost_read_texts", async move {
            let rows = sqlx::query!(
                r#"SELECT s.queryid AS "queryid!", s.query
                   FROM pg_stat_statements(true) s
                   WHERE s.toplevel
                     AND s.queryid = ANY($1::bigint[])
                     AND s.userid = (SELECT r.oid FROM pg_catalog.pg_roles r WHERE r.rolname = current_user)
                     AND s.dbid = (SELECT d.oid FROM pg_catalog.pg_database d WHERE d.datname = current_database())"#,
                queryids,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .filter_map(|row| row.query.map(|text| (row.queryid, text)))
                .collect())
        })
        .await
    }

    pub async fn count_requests_between(
        self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<i64, PersistError> {
        crate::metrics::time_db("db_cost_count_requests", async move {
            let count = sqlx::query_scalar!(
                r#"SELECT count(*)::bigint AS "count!"
                   FROM requests.requests r
                   WHERE r.created_at > $1 AND r.created_at <= $2"#,
                from,
                to,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(count)
        })
        .await
    }
}
