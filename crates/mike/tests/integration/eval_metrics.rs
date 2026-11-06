#[path = "eval_metrics/alarm_backlog.rs"]
mod alarm_backlog;
#[path = "eval_metrics/alarm_promotion.rs"]
mod alarm_promotion;
#[path = "eval_metrics/alarms.rs"]
mod alarms;
#[path = "eval_metrics/migration.rs"]
mod migration;
#[path = "eval_metrics/reads.rs"]
mod reads;
#[path = "eval_metrics/rest.rs"]
mod rest;
#[path = "eval_metrics/rollup.rs"]
mod rollup;

use crate::turn_facts::Fixture;

pub(crate) async fn set_setting(
    f: &Fixture,
    key: &str,
    value: serde_json::Value,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO config.settings (key, value) VALUES ($1, $2)
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
    )
    .bind(key)
    .bind(value)
    .execute(f.handle.pool())
    .await?;
    Ok(())
}

pub(crate) async fn rollup_count(f: &Fixture) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar("SELECT count(*) FROM eval.rollups")
        .fetch_one(f.handle.pool())
        .await?)
}
