use sqlx::AssertSqlSafe;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

pub(super) async fn connect(url: &str) -> anyhow::Result<PgPool> {
    Ok(PgPoolOptions::new().max_connections(4).connect(url).await?)
}

pub(super) async fn count(pool: &PgPool, table: &str) -> anyhow::Result<i64> {
    let n: i64 = sqlx::query_scalar(AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(pool)
        .await?;
    Ok(n)
}

pub(super) const BASELINE_TABLES: &[&str] = &[
    "system_prompts.system_prompts",
    "overrides.revisions",
    "overrides.overrides",
];

pub(super) async fn baseline_cursors(pool: &PgPool, tables: &[&str]) -> anyhow::Result<()> {
    for &table in tables {
        let max_id: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT COALESCE(max(id), 0) FROM {table}"
        )))
        .fetch_one(pool)
        .await?;
        sqlx::query(
            "INSERT INTO sync.cursor (table_name, last_local_id) VALUES ($1, $2)
             ON CONFLICT (table_name) DO UPDATE SET last_local_id = EXCLUDED.last_local_id",
        )
        .bind(table)
        .bind(max_id)
        .execute(pool)
        .await?;
    }
    Ok(())
}
