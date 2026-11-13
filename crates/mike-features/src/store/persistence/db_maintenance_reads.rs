use chrono::{DateTime, Utc};
use serde::Serialize;

use super::PersistError;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DatabaseMaintenance {
    pub taken_at: DateTime<Utc>,
    pub tables: Vec<TableMaintenance>,
    pub largest_indexes: Vec<IndexUsage>,
    pub cluster_io: Vec<IoUsage>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TableMaintenance {
    pub schema: String,
    pub table: String,
    pub dead_tuples: i64,
    pub total_autovacuum_ms: f64,
    pub stats_reset: Option<DateTime<Utc>>,
    pub autovacuum_score: Option<f64>,
    pub do_vacuum: Option<bool>,
    pub do_analyze: Option<bool>,
    pub for_wraparound: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IndexUsage {
    pub schema: String,
    pub table: String,
    pub index: String,
    pub bytes: i64,
    pub scans: i64,
    pub stats_reset: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IoUsage {
    pub backend_type: String,
    pub object: String,
    pub context: String,
    pub read_bytes: Option<i64>,
    pub stats_reset: Option<DateTime<Utc>>,
}

impl crate::store::persistence::state_repo::StateDb<'_> {
    pub async fn read_database_maintenance(
        self,
    ) -> Result<DatabaseMaintenance, PersistError> {
        crate::metrics::time_db("db_cost_read_maintenance", async {
            let tables = sqlx::query_as!(
                TableMaintenance,
                r#"SELECT t.schemaname::text AS "schema!", t.relname::text AS "table!",
                          t.n_dead_tup AS "dead_tuples!",
                          t.total_autovacuum_time AS "total_autovacuum_ms!", t.stats_reset,
                          a.score AS "autovacuum_score?", a.do_vacuum AS "do_vacuum?",
                          a.do_analyze AS "do_analyze?", a.for_wraparound AS "for_wraparound?"
                     FROM pg_stat_user_tables t
                     LEFT JOIN pg_stat_autovacuum_scores a ON a.relid = t.relid
                    ORDER BY t.n_dead_tup DESC, t.relid
                    LIMIT 50"#,
            )
            .fetch_all(&self.pool)
            .await?;
            let largest_indexes = sqlx::query_as!(
                IndexUsage,
                r#"SELECT schemaname::text AS "schema!", relname::text AS "table!",
                          indexrelname::text AS "index!",
                          pg_relation_size(indexrelid) AS "bytes!",
                          idx_scan AS "scans!", stats_reset
                     FROM pg_stat_user_indexes
                    ORDER BY pg_relation_size(indexrelid) DESC, indexrelid
                    LIMIT 30"#,
            )
            .fetch_all(&self.pool)
            .await?;
            let cluster_io = sqlx::query_as!(
                IoUsage,
                r#"SELECT backend_type AS "backend_type!", object AS "object!",
                          context AS "context!", read_bytes::bigint, stats_reset
                     FROM pg_stat_io ORDER BY backend_type, object, context"#,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(DatabaseMaintenance {
                taken_at: Utc::now(),
                tables,
                largest_indexes,
                cluster_io,
            })
        })
        .await
    }
}
