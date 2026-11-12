use std::time::Duration;

use serde_json::Value;
use sqlx::postgres::{PgRow, PgTypeInfo};
use sqlx::{
    AssertSqlSafe, Column, Executor, FromRow, PgPool, Postgres, SqlSafeStr, Statement, Transaction,
    TypeInfo,
};

use crate::wright::Finding;

const SET_DEADLINE: &str = "SET LOCAL statement_timeout = '10s'";
const OUTER_DEADLINE: Duration = Duration::from_secs(15);
const FINDING_COLUMNS: [&str; 3] = ["TEXT", "TEXT", "JSONB"];

async fn begin_read_only(pool: &PgPool) -> Result<Transaction<'static, Postgres>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION READ ONLY")
        .execute(&mut *tx)
        .await?;
    sqlx::query(SET_DEADLINE).execute(&mut *tx).await?;
    Ok(tx)
}

async fn read_only_rows<T>(pool: &PgPool, sql: &str, params: &Value) -> Result<Vec<T>, sqlx::Error>
where
    T: for<'r> FromRow<'r, PgRow> + Send + Unpin,
{
    let mut tx = begin_read_only(pool).await?;
    let rows = sqlx::query_as::<_, T>(AssertSqlSafe(sql.to_owned()))
        .bind(params)
        .fetch_all(&mut *tx)
        .await?;
    tx.rollback().await?;
    Ok(rows)
}

fn deadline_message(check: &str) -> String {
    format!("{check}: no answer within {}s", OUTER_DEADLINE.as_secs())
}

pub(crate) async fn rows<T>(
    pool: &PgPool,
    check: &str,
    sql: &str,
    params: &Value,
) -> Result<Vec<T>, String>
where
    T: for<'r> FromRow<'r, PgRow> + Send + Unpin,
{
    match tokio::time::timeout(OUTER_DEADLINE, read_only_rows(pool, sql, params)).await {
        Ok(Ok(rows)) => Ok(rows),
        Ok(Err(e)) => Err(format!("{check}: query failed: {e}")),
        Err(_) => Err(deadline_message(check)),
    }
}

pub(crate) async fn findings(
    pool: &PgPool,
    check: &str,
    sql: &str,
    params: &Value,
) -> Result<Vec<Finding>, String> {
    let rows: Vec<(String, String, Value)> = rows(pool, check, sql, params).await?;
    Ok(rows
        .into_iter()
        .map(|(scope, resource, evidence)| Finding {
            scope,
            resource,
            evidence,
        })
        .collect())
}

async fn run_for_shape(pool: &PgPool, sql: &str, params: &Value) -> Result<(), String> {
    let mut tx = begin_read_only(pool).await.map_err(|e| e.to_string())?;
    let statement = (&mut *tx)
        .prepare_with(
            AssertSqlSafe(sql.to_owned()).into_sql_str(),
            &[PgTypeInfo::with_name("jsonb")],
        )
        .await
        .map_err(|e| e.to_string())?;
    let columns: Vec<String> = statement
        .columns()
        .iter()
        .map(|c| c.type_info().name().to_ascii_uppercase())
        .collect();
    if columns != FINDING_COLUMNS {
        return Err(format!(
            "the query returns columns of type ({}); it must return (text, text, jsonb) as (scope, resource, evidence)",
            columns.join(", ").to_ascii_lowercase()
        ));
    }
    sqlx::query_as::<_, (String, String, Value)>(AssertSqlSafe(sql.to_owned()))
        .bind(params)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    tx.rollback().await.map_err(|e| e.to_string())
}

pub(crate) async fn validate(pool: &PgPool, sql: &str, params: &Value) -> Result<(), String> {
    match tokio::time::timeout(OUTER_DEADLINE, run_for_shape(pool, sql, params)).await {
        Ok(result) => result,
        Err(_) => Err(deadline_message("query")),
    }
}
