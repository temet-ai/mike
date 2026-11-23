use serde_json::Value;
use sqlx::PgPool;

const CHECKS: [&str; 2] = ["idle_polls", "reads_outpace_writes"];
const TAKE: &str = include_str!("counters_take.sql");
const PRUNE: &str = include_str!("counters_prune.sql");
const TABLES_TAKE: &str = include_str!("table_counters_take.sql");
const TABLES_PRUNE: &str = include_str!("table_counters_prune.sql");
const GONE_AFTER_HOURS: i32 = 6;
const SET_DEADLINE: &str = "SET LOCAL statement_timeout = '10s'";

pub(crate) fn reads_counters(check: &str) -> bool {
    CHECKS.contains(&check)
}

pub(crate) async fn save(pool: &PgPool, check: &str, params: &Value) -> Result<(), String> {
    take(pool, params)
        .await
        .map_err(|e| format!("{check}: saving the statement counters failed: {e}"))
}

async fn take(pool: &PgPool, params: &Value) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(SET_DEADLINE).execute(&mut *tx).await?;
    sqlx::query(TAKE).bind(params).execute(&mut *tx).await?;
    sqlx::query(TABLES_TAKE).execute(&mut *tx).await?;
    sqlx::query(PRUNE)
        .bind(GONE_AFTER_HOURS)
        .execute(&mut *tx)
        .await?;
    sqlx::query(TABLES_PRUNE).execute(&mut *tx).await?;
    tx.commit().await
}
