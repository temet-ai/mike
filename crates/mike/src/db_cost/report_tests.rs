use chrono::{Duration, TimeZone};

use super::*;
use crate::db_cost::budget::Tier;
use crate::db_cost::snapshot::Counters;

fn window(secs: i64, deltas: Vec<(i64, Counters)>) -> anyhow::Result<Window> {
    let from = Utc
        .timestamp_opt(1_800_000_000, 0)
        .single()
        .ok_or_else(|| anyhow::anyhow!("timestamp out of range"))?;
    Ok(Window {
        from,
        to: from + Duration::seconds(secs),
        stats_reset: None,
        deltas,
    })
}

fn delta(calls: i64, exec_ms: f64) -> Counters {
    Counters {
        calls,
        exec_ms,
        ..Counters::default()
    }
}

#[test]
fn build_cheap_window_is_ok_with_statements_per_request() -> anyhow::Result<()> {
    let config = DbCostConfig::default();
    let window = window(3600, vec![(1, delta(600, 600.0)), (2, delta(400, 2000.0))])?;
    let report = build(&window, Some(50), &config, Utc::now());
    assert_eq!(report.status, ReportStatus::Ok);
    assert_eq!(report.total_calls, 1000);
    assert_eq!(report.statements_per_request, Some(20.0));
    assert!((report.db_secs_per_hour - 2.6).abs() < 1e-9);
    assert!(report.offenders.is_empty());
    assert!(report.fix.is_empty());
    assert_eq!(
        report.top_by_db_time.first().map(|c| c.queryid.as_str()),
        Some("2")
    );
    Ok(())
}

#[test]
fn build_slow_hot_poll_is_degraded_and_names_it() -> anyhow::Result<()> {
    let config = DbCostConfig::default();
    let window = window(
        3600,
        vec![(1, delta(600, 600.0)), (-42, delta(7200, 7200.0 * 60.0))],
    )?;
    let report = build(&window, None, &config, Utc::now());
    assert_eq!(report.status, ReportStatus::Degraded);
    assert_eq!(report.offenders.len(), 1);
    let worst = report
        .offenders
        .first()
        .ok_or_else(|| anyhow::anyhow!("no offender"))?;
    assert_eq!(worst.queryid, "-42");
    assert_eq!(worst.tier, Tier::Hot);
    assert!(report.detail.contains("statement -42"), "{}", report.detail);
    assert!(
        report.detail.contains("hot budget 5 ms"),
        "{}",
        report.detail
    );
    assert!(report.fix.contains("/api/db-cost"), "{}", report.fix);
    assert_eq!(report.statements_per_request, None);
    Ok(())
}

#[test]
fn build_total_over_alarm_degrades_without_a_single_offender() -> anyhow::Result<()> {
    let config = DbCostConfig {
        total_alarm_db_secs_per_hour: 100.0,
        ..DbCostConfig::default()
    };
    let deltas = (1..=5).map(|id| (id, delta(100_000, 50_000.0))).collect();
    let report = build(&window(3600, deltas)?, Some(0), &config, Utc::now());
    assert!(report.offenders.is_empty());
    assert!(report.total_over_alarm);
    assert_eq!(report.status, ReportStatus::Degraded);
    assert_eq!(report.statements_per_request, None);
    assert!(report.detail.contains("costliest"), "{}", report.detail);
    Ok(())
}

#[test]
fn build_keeps_every_offender_and_caps_only_the_ranking() -> anyhow::Result<()> {
    let config = DbCostConfig {
        top_statements: 2,
        ..DbCostConfig::default()
    };
    let deltas = (1..=5).map(|id| (id, delta(360, 360.0 * 400.0))).collect();
    let report = build(&window(3600, deltas)?, None, &config, Utc::now());
    assert_eq!(report.offenders.len(), 5);
    assert_eq!(report.top_by_db_time.len(), 2);
    assert_eq!(report.statements_active, 5);
    Ok(())
}

#[test]
fn attach_texts_fills_offenders_and_ranking_with_full_text() -> anyhow::Result<()> {
    let config = DbCostConfig::default();
    let window = window(3600, vec![(-42, delta(7200, 7200.0 * 60.0))])?;
    let mut report = build(&window, None, &config, Utc::now());
    assert_eq!(wanted_texts(&report), vec![-42]);
    let text = "SELECT * FROM mailbox.threads WHERE id = $1".to_owned();
    attach_texts(&mut report, &HashMap::from([(-42, text.clone())]));
    assert!(
        report
            .offenders
            .iter()
            .chain(report.top_by_db_time.iter())
            .all(|cost| cost.query.as_deref() == Some(text.as_str()))
    );
    Ok(())
}

#[test]
fn missing_extension_is_unavailable_with_the_setup_fix() {
    let report = missing_extension(Utc::now(), &DbCostConfig::default());
    assert_eq!(report.status, ReportStatus::Unavailable);
    assert!(report.fix.contains("scripts/capture-db/up.sh"));
}

#[test]
fn read_failed_carries_the_error_text() {
    let report = read_failed(
        Utc::now(),
        &DbCostConfig::default(),
        &"pg_stat_statements must be loaded via shared_preload_libraries",
    );
    assert_eq!(report.status, ReportStatus::Unavailable);
    assert!(report.detail.contains("shared_preload_libraries"));
}
