use serde_json::Value;
use sqlx::PgPool;

pub(crate) const CHECK: &str = "idle_polls";
const TAKE: &str = include_str!("counters_take.sql");
const PRUNE: &str = include_str!("counters_prune.sql");
const GONE_AFTER_HOURS: i32 = 6;
const SET_DEADLINE: &str = "SET LOCAL statement_timeout = '10s'";

pub(crate) async fn save(pool: &PgPool, params: &Value) -> Result<(), String> {
    take(pool, params)
        .await
        .map_err(|e| format!("{CHECK}: saving the statement counters failed: {e}"))
}

async fn take(pool: &PgPool, params: &Value) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(SET_DEADLINE).execute(&mut *tx).await?;
    sqlx::query(TAKE).bind(params).execute(&mut *tx).await?;
    sqlx::query(PRUNE)
        .bind(GONE_AFTER_HOURS)
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}
