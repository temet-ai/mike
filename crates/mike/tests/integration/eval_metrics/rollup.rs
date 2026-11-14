use chrono::NaiveDate;
use mike::eval_metrics::EVAL_ROLLUP_SQL_VERSION;
use mike::store::persistence::eval_metrics::RollupRequest;
use mike::store::persistence::eval_repo::EvalPersist as _;
use mike::store::persistence::improvement::effect::ACTION_EFFECT_SQL_VERSION;

use super::{rollup_count, set_setting};
use crate::turn_facts::fixture;

fn request(execute: bool, version: &str) -> RollupRequest<'_> {
    RollupRequest {
        for_day: NaiveDate::from_ymd_opt(2026, 9, 20).unwrap_or_default(),
        mode: "manual",
        sql_version: version,
        execute,
    }
}

#[tokio::test]
async fn rollup_same_day_twice_writes_one_rollup() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let (a, b) = tokio::join!(
        f.handle
            .eval()
            .run_eval_rollup(request(true, EVAL_ROLLUP_SQL_VERSION)),
        f.handle
            .eval()
            .run_eval_rollup(request(true, EVAL_ROLLUP_SQL_VERSION)),
    );
    let (a, b) = (a?, b?);
    assert_eq!(u8::from(a.already_rolled) + u8::from(b.already_rolled), 1);
    assert_eq!(rollup_count(&f).await?, 1);
    Ok(())
}

#[tokio::test]
async fn rollup_new_sql_version_rerolls_day() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    f.handle
        .eval()
        .run_eval_rollup(request(true, "v-a"))
        .await?;
    f.handle
        .eval()
        .run_eval_rollup(request(true, "v-b"))
        .await?;
    assert_eq!(rollup_count(&f).await?, 2);
    Ok(())
}

#[tokio::test]
async fn rollup_dry_run_writes_nothing() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let report = f
        .handle
        .eval()
        .run_eval_rollup(request(false, EVAL_ROLLUP_SQL_VERSION))
        .await?;
    assert!(!report.executed);
    assert_eq!(report.per_family.len(), 5);
    assert_eq!(rollup_count(&f).await?, 0);
    Ok(())
}

#[tokio::test]
async fn rollup_disabled_writes_nothing_on_the_next_tick() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    mike::features_settings::register();
    set_setting(&f, "eval.rollup.enabled", serde_json::json!(false)).await?;
    set_setting(&f, "eval.rollup.hour_utc", serde_json::json!(0)).await?;
    let disabled = mike::eval_metrics::run_tick(&f.handle, &["turn_judge".to_owned()], false).await;
    assert!(disabled);
    assert_eq!(rollup_count(&f).await?, 0);
    let alarms: i64 = sqlx::query_scalar("SELECT count(*) FROM eval.alarms")
        .fetch_one(f.handle.pool())
        .await?;
    assert_eq!(alarms, 0);
    Ok(())
}

async fn rollups_of(f: &crate::turn_facts::Fixture, sql_version: &str) -> anyhow::Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT count(*) FROM eval.rollups WHERE sql_version = $1")
            .bind(sql_version)
            .fetch_one(f.handle.pool())
            .await?,
    )
}

#[tokio::test]
async fn an_enabled_tick_past_the_hour_writes_each_rollup_once() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    set_setting(&f, "eval.rollup.hour_utc", serde_json::json!(0)).await?;
    let disabled = mike::eval_metrics::run_tick(&f.handle, &[], false).await;
    assert!(!disabled);
    assert_eq!(rollups_of(&f, EVAL_ROLLUP_SQL_VERSION).await?, 1);
    assert_eq!(rollups_of(&f, ACTION_EFFECT_SQL_VERSION).await?, 1);
    mike::eval_metrics::run_tick(&f.handle, &[], false).await;
    assert_eq!(rollups_of(&f, EVAL_ROLLUP_SQL_VERSION).await?, 1);
    assert_eq!(rollups_of(&f, ACTION_EFFECT_SQL_VERSION).await?, 1);
    assert_eq!(rollup_count(&f).await?, 2);
    Ok(())
}
