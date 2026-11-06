use std::collections::HashMap;

use chrono::{DateTime, TimeZone};

use super::*;
use crate::config::db_cost::DbCostConfig;
use crate::db_cost::report;
use crate::db_cost::snapshot::{Counters, Snapshot};
use crate::db_cost::worker::Monitor;

fn at(secs: i64) -> anyhow::Result<DateTime<Utc>> {
    Utc.timestamp_opt(1_800_000_000 + secs, 0)
        .single()
        .ok_or_else(|| anyhow::anyhow!("timestamp out of range"))
}

fn snapshot(taken: i64, calls: i64, exec_ms: f64) -> anyhow::Result<Snapshot> {
    Ok(Snapshot {
        taken_at: at(taken)?,
        stats_reset: Some(at(-1000)?),
        statements: HashMap::from([(
            77,
            Counters {
                calls,
                exec_ms,
                ..Counters::default()
            },
        )]),
    })
}

fn next_report(monitor: &mut Monitor, snap: Snapshot) -> anyhow::Result<DbCostReport> {
    let now = snap.taken_at;
    let window = monitor
        .observe(snap)
        .ok_or_else(|| anyhow::anyhow!("no window yet"))?;
    let built = report::build(&window, Some(100), monitor.config(), now);
    Ok(monitor.finish(built, HashMap::new()))
}

#[test]
fn health_slow_window_degrades_then_recovers_when_it_slides_out() -> anyhow::Result<()> {
    let mut monitor = Monitor::new(DbCostConfig {
        window_secs: 600,
        ..DbCostConfig::default()
    });
    assert_eq!(monitor.observe(snapshot(0, 0, 0.0)?), None);

    let slow = next_report(&mut monitor, snapshot(300, 600, 600.0 * 80.0)?)?;
    assert_eq!(slow.status, ReportStatus::Degraded);
    assert_eq!(apply_alarm("ok", Some(&slow)), "degraded");
    let row = capability_row(true, Some(&slow));
    assert_eq!(row.id, "db_cost");
    assert_eq!(row.state, CapabilityState::Degraded);
    assert!(row.detail.contains("statement 77"), "{}", row.detail);
    assert!(!row.fix.is_empty());
    let section = health_section(Some(&slow)).ok_or_else(|| anyhow::anyhow!("no section"))?;
    assert_eq!(section.offender_count, 1);

    next_report(&mut monitor, snapshot(600, 1200, 600.0 * 80.0 + 600.0)?)?;
    let healed = next_report(&mut monitor, snapshot(900, 1800, 600.0 * 80.0 + 1200.0)?)?;
    assert_eq!(healed.status, ReportStatus::Ok, "{}", healed.detail);
    assert_eq!(apply_alarm("ok", Some(&healed)), "ok");
    let row = capability_row(true, Some(&healed));
    assert_eq!(row.state, CapabilityState::Active);
    assert!(row.fix.is_empty());
    Ok(())
}

#[test]
fn apply_alarm_never_clears_another_subsystems_degrade() -> anyhow::Result<()> {
    let config = DbCostConfig::default();
    let warming = report::warming_up(at(0)?, &config);
    assert_eq!(apply_alarm("degraded", Some(&warming)), "degraded");
    assert_eq!(apply_alarm("ok", None), "ok");
    Ok(())
}

#[test]
fn capability_row_without_persistence_is_off() {
    let row = capability_row(false, None);
    assert_eq!(row.state, CapabilityState::Off);
    assert!(health_section(None).is_none());
}

#[test]
fn capability_row_missing_extension_is_off_with_its_fix() -> anyhow::Result<()> {
    let missing = report::missing_extension(at(0)?, &DbCostConfig::default());
    let row = capability_row(true, Some(&missing));
    assert_eq!(row.state, CapabilityState::Off);
    assert!(row.fix.contains("pg_stat_statements"), "{}", row.fix);
    Ok(())
}

#[test]
fn capability_row_warming_up_is_active() -> anyhow::Result<()> {
    let warming = report::warming_up(at(0)?, &DbCostConfig::default());
    assert_eq!(
        capability_row(true, Some(&warming)).state,
        CapabilityState::Active
    );
    Ok(())
}
