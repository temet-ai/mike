use std::collections::HashMap;

use chrono::{DateTime, Duration, TimeZone, Utc};

use super::*;

fn at(secs: i64) -> anyhow::Result<DateTime<Utc>> {
    Utc.timestamp_opt(1_800_000_000 + secs, 0)
        .single()
        .ok_or_else(|| anyhow::anyhow!("timestamp out of range"))
}

fn counters(calls: i64, exec_ms: f64, blks_read: i64) -> Counters {
    Counters {
        calls,
        generic_plan_calls: calls / 2,
        custom_plan_calls: calls - calls / 2,
        exec_ms,
        blks_hit: calls * 10,
        blks_read,
        blks_dirtied: 0,
        blks_written: 0,
    }
}

fn snapshot(taken: i64, reset: i64, statements: &[(i64, Counters)]) -> anyhow::Result<Snapshot> {
    Ok(Snapshot {
        taken_at: at(taken)?,
        stats_reset: Some(at(reset)?),
        statements: statements.iter().copied().collect::<HashMap<_, _>>(),
    })
}

fn delta_of(window: &Window, id: i64) -> Option<Counters> {
    window
        .deltas
        .iter()
        .find(|(queryid, _)| *queryid == id)
        .map(|(_, delta)| *delta)
}

#[test]
fn window_first_snapshot_is_none() -> anyhow::Result<()> {
    let mut ring = SnapshotRing::new(3600);
    ring.push(snapshot(0, -100, &[(1, counters(10, 50.0, 1))])?);
    assert_eq!(ring.window(), None);
    Ok(())
}

#[test]
fn window_two_snapshots_diffs_every_counter() -> anyhow::Result<()> {
    let mut ring = SnapshotRing::new(3600);
    ring.push(snapshot(0, -100, &[(1, counters(10, 50.0, 1))])?);
    ring.push(snapshot(300, -100, &[(1, counters(40, 350.0, 7))])?);
    let window = ring.window().ok_or_else(|| anyhow::anyhow!("no window"))?;
    assert_eq!(window.from, at(0)?);
    assert_eq!(window.to, at(300)?);
    assert!((window.secs() - 300.0).abs() < f64::EPSILON);
    let Some(delta) = delta_of(&window, 1) else {
        anyhow::bail!("statement 1 missing from {window:?}");
    };
    assert_eq!(delta.calls, 30);
    assert_eq!(delta.generic_plan_calls, 15);
    assert_eq!(delta.custom_plan_calls, 15);
    assert!((delta.exec_ms - 300.0).abs() < 1e-9);
    assert_eq!(delta.blks_hit, 300);
    assert_eq!(delta.blks_read, 6);
    Ok(())
}

#[test]
fn window_statement_without_new_calls_is_left_out() -> anyhow::Result<()> {
    let mut ring = SnapshotRing::new(3600);
    let idle = counters(5, 5.0, 0);
    ring.push(snapshot(0, -100, &[(1, idle), (2, counters(1, 1.0, 0))])?);
    ring.push(snapshot(300, -100, &[(1, idle), (2, counters(3, 4.0, 0))])?);
    let window = ring.window().ok_or_else(|| anyhow::anyhow!("no window"))?;
    assert_eq!(delta_of(&window, 1), None);
    assert_eq!(delta_of(&window, 2).map(|d| d.calls), Some(2));
    Ok(())
}

#[test]
fn window_new_statement_counts_in_full() -> anyhow::Result<()> {
    let mut ring = SnapshotRing::new(3600);
    ring.push(snapshot(0, -100, &[])?);
    ring.push(snapshot(300, -100, &[(9, counters(4, 80.0, 2))])?);
    let window = ring.window().ok_or_else(|| anyhow::anyhow!("no window"))?;
    assert_eq!(delta_of(&window, 9), Some(counters(4, 80.0, 2)));
    Ok(())
}

#[test]
fn window_evicted_statement_with_fewer_calls_counts_current_values() -> anyhow::Result<()> {
    let mut ring = SnapshotRing::new(3600);
    ring.push(snapshot(0, -100, &[(1, counters(500, 900.0, 3))])?);
    ring.push(snapshot(300, -100, &[(1, counters(20, 40.0, 1))])?);
    let window = ring.window().ok_or_else(|| anyhow::anyhow!("no window"))?;
    assert_eq!(delta_of(&window, 1), Some(counters(20, 40.0, 1)));
    Ok(())
}

#[test]
fn window_after_stats_reset_starts_at_the_reset() -> anyhow::Result<()> {
    let mut ring = SnapshotRing::new(3600);
    ring.push(snapshot(0, -100, &[(1, counters(1000, 9000.0, 50))])?);
    ring.push(snapshot(300, -100, &[(1, counters(1100, 9900.0, 55))])?);
    ring.push(snapshot(600, 450, &[(1, counters(30, 60.0, 2))])?);
    let window = ring.window().ok_or_else(|| anyhow::anyhow!("no window"))?;
    assert_eq!(window.from, at(450)?);
    assert_eq!(window.to, at(600)?);
    assert_eq!(window.stats_reset, Some(at(450)?));
    assert_eq!(delta_of(&window, 1), Some(counters(30, 60.0, 2)));
    Ok(())
}

#[test]
fn window_after_stats_reset_keeps_the_reset_baseline_on_the_next_tick() -> anyhow::Result<()> {
    let mut ring = SnapshotRing::new(3600);
    ring.push(snapshot(0, -100, &[(1, counters(1000, 9000.0, 50))])?);
    ring.push(snapshot(600, 450, &[(1, counters(30, 60.0, 2))])?);
    ring.push(snapshot(900, 450, &[(1, counters(50, 100.0, 2))])?);
    let window = ring.window().ok_or_else(|| anyhow::anyhow!("no window"))?;
    assert_eq!(window.from, at(450)?);
    assert_eq!(delta_of(&window, 1).map(|d| d.calls), Some(50));
    Ok(())
}

#[test]
fn window_slides_once_older_snapshots_leave_the_span() -> anyhow::Result<()> {
    let mut ring = SnapshotRing::new(600);
    for (step, calls) in [0_i64, 10, 20, 30].iter().enumerate() {
        let taken = i64::try_from(step)? * 300;
        ring.push(snapshot(
            taken,
            -100,
            &[(1, counters(*calls, *calls as f64, 0))],
        )?);
    }
    let window = ring.window().ok_or_else(|| anyhow::anyhow!("no window"))?;
    assert_eq!(window.from, at(300)?);
    assert_eq!(window.to, at(900)?);
    assert_eq!(window.to - window.from, Duration::seconds(600));
    assert_eq!(delta_of(&window, 1).map(|d| d.calls), Some(20));
    Ok(())
}
