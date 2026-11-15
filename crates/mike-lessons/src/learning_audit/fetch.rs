use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::{Map, Value, json};
use sqlx::PgPool;

use super::queries::{Binds, SPECS, Spec};

const STATEMENT_TIMEOUT_MS: &str = "300000";
const LOCK_TIMEOUT_MS: &str = "2000";
const MISSING_OBJECT_CODES: [&str; 3] = ["42P01", "42703", "3F000"];

#[derive(Debug, Clone, Copy)]
pub(super) struct Window {
    pub(super) start: DateTime<Utc>,
    pub(super) end: DateTime<Utc>,
}

pub(super) fn is_missing_object_code(code: &str) -> bool {
    MISSING_OBJECT_CODES.contains(&code)
}

fn is_missing_object(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|database| database.code())
        .is_some_and(|code| is_missing_object_code(&code))
}

pub(super) fn recall_markers() -> [Value; 4] {
    [
        json!({"stages": [{"stage": "recall_injector"}]}),
        json!({"stages": [{"stage": "recall_injector", "reason_code": "recall_no_tags"}]}),
        json!({"stages": [{"stage": "recall_injector", "reason_code": "recall_lessons_injected"}]}),
        json!({"stages": [{"stage": "recall_injector", "reason_code": "recall_delivered_out_of_band"}]}),
    ]
}

async fn fetch_rows(
    tx: &mut sqlx::PgConnection,
    spec: &Spec,
    window: &Window,
) -> Result<Vec<Value>, sqlx::Error> {
    let query = sqlx::query_scalar::<_, Value>(spec.sql);
    match spec.binds {
        Binds::None => query.fetch_all(tx).await,
        Binds::Start => query.bind(window.start).fetch_all(tx).await,
        Binds::Window => {
            query
                .bind(window.start)
                .bind(window.end)
                .fetch_all(tx)
                .await
        }
        Binds::Recall => {
            let [stage, no_tags, injected, out_of_band] = recall_markers();
            query
                .bind(window.start)
                .bind(window.end)
                .bind(stage)
                .bind(no_tags)
                .bind(injected)
                .bind(out_of_band)
                .fetch_all(tx)
                .await
        }
    }
}

async fn run_spec(pool: &PgPool, spec: &Spec, window: &Window) -> Result<Vec<Value>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION READ ONLY")
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "SELECT set_config('statement_timeout', $1, true), set_config('lock_timeout', $2, true)",
    )
    .bind(STATEMENT_TIMEOUT_MS)
    .bind(LOCK_TIMEOUT_MS)
    .execute(&mut *tx)
    .await?;
    let rows = fetch_rows(&mut tx, spec, window).await?;
    tx.rollback().await?;
    Ok(rows)
}

pub(super) async fn run_queries(pool: &PgPool, window: &Window) -> Result<Value> {
    let mut data = Map::new();
    for spec in SPECS {
        let value = match run_spec(pool, spec, window).await {
            Ok(rows) => Value::Array(rows),
            Err(error) if spec.optional && is_missing_object(&error) => Value::Null,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("learning audit query {:?} failed", spec.name));
            }
        };
        data.insert(spec.name.to_owned(), value);
    }
    Ok(Value::Object(data))
}
