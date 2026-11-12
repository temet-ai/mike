use std::time::Duration;

use serde_json::Value;
use sqlx::postgres::PgRow;
use sqlx::{FromRow, PgPool};

use crate::wright::Finding;

const SET_DEADLINE: &str = "SET LOCAL statement_timeout = '10s'";
const OUTER_DEADLINE: Duration = Duration::from_secs(15);

async fn read_only_rows<T>(pool: &PgPool, sql: &'static str) -> Result<Vec<T>, sqlx::Error>
where
    T: for<'r> FromRow<'r, PgRow> + Send + Unpin,
{
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION READ ONLY")
        .execute(&mut *tx)
        .await?;
    sqlx::query(SET_DEADLINE).execute(&mut *tx).await?;
    let rows = sqlx::query_as::<_, T>(sql).fetch_all(&mut *tx).await?;
    tx.rollback().await?;
    Ok(rows)
}

pub(crate) async fn rows<T>(pool: &PgPool, check: &str, sql: &'static str) -> Result<Vec<T>, String>
where
    T: for<'r> FromRow<'r, PgRow> + Send + Unpin,
{
    match tokio::time::timeout(OUTER_DEADLINE, read_only_rows(pool, sql)).await {
        Ok(Ok(rows)) => Ok(rows),
        Ok(Err(e)) => Err(format!("{check}: query failed: {e}")),
        Err(_) => Err(format!(
            "{check}: no answer within {}s",
            OUTER_DEADLINE.as_secs()
        )),
    }
}

pub(crate) async fn findings(
    pool: &PgPool,
    check: &str,
    sql: &'static str,
) -> Result<Vec<Finding>, String> {
    let rows: Vec<(String, String, Value)> = rows(pool, check, sql).await?;
    Ok(rows
        .into_iter()
        .map(|(scope, resource, evidence)| Finding {
            scope,
            resource,
            evidence,
        })
        .collect())
}
