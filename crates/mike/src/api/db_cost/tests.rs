use chrono::{Duration, TimeZone, Utc};

use super::*;
use crate::config::db_cost::DbCostConfig;
use crate::db_cost::report::{ReportStatus, build};
use crate::db_cost::snapshot::{Counters, Window};

fn degraded_report() -> anyhow::Result<DbCostReport> {
    let from = Utc
        .timestamp_opt(1_800_000_000, 0)
        .single()
        .ok_or_else(|| anyhow::anyhow!("timestamp out of range"))?;
    let window = Window {
        from,
        to: from + Duration::hours(1),
        stats_reset: None,
        deltas: vec![(
            -42,
            Counters {
                calls: 7200,
                exec_ms: 7200.0 * 60.0,
                ..Counters::default()
            },
        )],
    };
    Ok(build(&window, Some(360), &DbCostConfig::default(), from))
}

#[tokio::test]
async fn get_db_cost_without_persistence_is_not_monitored() -> anyhow::Result<()> {
    let state = AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
    let Json(body) = get_db_cost(State(state)).await;
    assert!(!body.monitored);
    Ok(())
}

#[test]
fn answer_carries_the_whole_report_with_every_offender() -> anyhow::Result<()> {
    let report = degraded_report()?;
    let body = serde_json::to_value(answer(true, Some(&report)))?;
    assert_eq!(body["monitored"], true);
    assert_eq!(body["report"]["status"], "degraded");
    assert_eq!(body["report"]["offenders"][0]["queryid"], "-42");
    assert_eq!(body["report"]["offenders"][0]["tier"], "hot");
    assert_eq!(body["report"]["statements_per_request"], 20.0);
    assert_eq!(report.status, ReportStatus::Degraded);
    Ok(())
}

#[test]
fn answer_before_the_first_report_is_null() -> anyhow::Result<()> {
    let body = serde_json::to_value(answer(true, None))?;
    assert!(body["report"].is_null());
    Ok(())
}
