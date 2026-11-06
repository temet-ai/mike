use chrono::{DateTime, TimeZone};

use super::*;
use crate::db_cost::snapshot::Counters;

fn at(secs: i64) -> anyhow::Result<DateTime<Utc>> {
    Utc.timestamp_opt(1_800_000_000 + secs, 0)
        .single()
        .ok_or_else(|| anyhow::anyhow!("timestamp out of range"))
}

fn snapshot(taken: i64, statements: &[(i64, i64, f64)]) -> anyhow::Result<Snapshot> {
    Ok(Snapshot {
        taken_at: at(taken)?,
        stats_reset: None,
        statements: statements
            .iter()
            .map(|(id, calls, exec_ms)| {
                (
                    *id,
                    Counters {
                        calls: *calls,
                        exec_ms: *exec_ms,
                        ..Counters::default()
                    },
                )
            })
            .collect(),
    })
}

fn report_for(monitor: &mut Monitor, snap: Snapshot) -> anyhow::Result<DbCostReport> {
    let window = monitor
        .observe(snap)
        .ok_or_else(|| anyhow::anyhow!("no window yet"))?;
    Ok(report::build(&window, None, monitor.config(), at(0)?))
}

#[test]
fn monitor_fetches_each_statement_text_once() -> anyhow::Result<()> {
    let mut monitor = Monitor::new(DbCostConfig::default());
    monitor.observe(snapshot(0, &[])?);
    let first = report_for(&mut monitor, snapshot(300, &[(5, 10, 10.0)])?)?;
    assert_eq!(monitor.missing_texts(&first), vec![5]);
    let first = monitor.finish(first, HashMap::from([(5, "SELECT 5".to_owned())]));
    assert_eq!(
        first
            .top_by_db_time
            .first()
            .and_then(|c| c.query.as_deref()),
        Some("SELECT 5")
    );

    let second = report_for(&mut monitor, snapshot(600, &[(5, 20, 20.0)])?)?;
    assert!(monitor.missing_texts(&second).is_empty());
    let second = monitor.finish(second, HashMap::new());
    assert_eq!(
        second
            .top_by_db_time
            .first()
            .and_then(|c| c.query.as_deref()),
        Some("SELECT 5")
    );
    Ok(())
}

#[test]
fn monitor_forgets_texts_of_statements_no_longer_reported() -> anyhow::Result<()> {
    let mut monitor = Monitor::new(DbCostConfig {
        top_statements: 1,
        window_secs: 300,
        ..DbCostConfig::default()
    });
    monitor.observe(snapshot(0, &[])?);
    let first = report_for(&mut monitor, snapshot(300, &[(5, 10, 10.0)])?)?;
    monitor.finish(first, HashMap::from([(5, "SELECT 5".to_owned())]));

    let second = report_for(
        &mut monitor,
        snapshot(600, &[(5, 10, 10.0), (6, 10, 900.0)])?,
    )?;
    assert_eq!(monitor.missing_texts(&second), vec![6]);
    monitor.finish(second, HashMap::from([(6, "SELECT 6".to_owned())]));

    let third = report_for(
        &mut monitor,
        snapshot(900, &[(5, 20, 20.0), (6, 10, 900.0)])?,
    )?;
    assert_eq!(monitor.missing_texts(&third), vec![5]);
    Ok(())
}
