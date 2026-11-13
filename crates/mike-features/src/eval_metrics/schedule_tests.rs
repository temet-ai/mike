use chrono::{NaiveDate, TimeZone, Utc};

use super::schedule::rollup_due;

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap_or_default()
}

#[test]
fn rollup_due_before_hour_returns_none() {
    let now = Utc
        .with_ymd_and_hms(2026, 9, 24, 2, 59, 0)
        .single()
        .unwrap_or_default();
    assert_eq!(rollup_due(now, 3, None), None);
}

#[test]
fn rollup_due_at_hour_without_rollup_returns_today() {
    let now = Utc
        .with_ymd_and_hms(2026, 9, 24, 3, 0, 0)
        .single()
        .unwrap_or_default();
    assert_eq!(
        rollup_due(now, 3, Some(day(2026, 9, 23))),
        Some(day(2026, 9, 24))
    );
}

#[test]
fn rollup_due_today_already_rolled_returns_none() {
    let now = Utc
        .with_ymd_and_hms(2026, 9, 24, 9, 0, 0)
        .single()
        .unwrap_or_default();
    assert_eq!(rollup_due(now, 3, Some(day(2026, 9, 24))), None);
}

#[test]
fn rollup_due_restart_past_hour_returns_today() {
    let now = Utc
        .with_ymd_and_hms(2026, 9, 24, 17, 0, 0)
        .single()
        .unwrap_or_default();
    assert_eq!(rollup_due(now, 3, None), Some(day(2026, 9, 24)));
}

#[test]
fn rollup_due_hour_23_boundary_uses_utc_date() {
    let now = Utc
        .with_ymd_and_hms(2026, 9, 24, 23, 30, 0)
        .single()
        .unwrap_or_default();
    assert_eq!(
        rollup_due(now, 23, Some(day(2026, 9, 23))),
        Some(day(2026, 9, 24))
    );
}
