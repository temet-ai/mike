use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgConnection;

use super::super::PersistError;

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct NewRate {
    pub family: &'static str,
    pub metric: &'static str,
    pub population: &'static str,
    pub scope_kind: &'static str,
    pub project_id: Option<i64>,
    pub window_start: Option<DateTime<Utc>>,
    pub window_end: DateTime<Utc>,
    pub window_days: Option<i32>,
    pub role: &'static str,
    pub bundle_label: Option<String>,
    pub axis: String,
    pub class: String,
    pub label_tier: Option<String>,
    pub label_split: Option<String>,
    pub label_source: Option<String>,
    pub dimension: Option<String>,
    pub dimension_value: Option<String>,
    pub steerer_id: Option<i64>,
    pub question: Option<String>,
    pub mode: Option<String>,
    pub k: i32,
    pub n: i32,
    pub detail: serde_json::Value,
}

pub(crate) async fn insert_rates(
    conn: &mut PgConnection,
    rollup_id: i64,
    rates: &[NewRate],
) -> Result<u64, PersistError> {
    if rates.is_empty() {
        return Ok(0);
    }
    let payload = serde_json::to_value(rates)?;
    Ok(sqlx::query_file!(
        "src/store/persistence/eval_metrics/insert_rates.sql",
        rollup_id,
        payload,
    )
    .execute(&mut *conn)
    .await?
    .rows_affected())
}
