use std::time::Duration;

use chrono::{DateTime, NaiveDate, Timelike, Utc};

pub(crate) const PASS_INTERVAL: Duration = Duration::from_secs(900);
pub(crate) const CLEAR_AFTER_QUIET_EVALUATIONS: i32 = 3;
pub const EVAL_ROLLUP_SQL_VERSION: &str = "jc18-v1";

pub fn rollup_due(
    now: DateTime<Utc>,
    hour_utc: u32,
    last_for_day: Option<NaiveDate>,
) -> Option<NaiveDate> {
    if now.hour() < hour_utc {
        return None;
    }
    let today = now.date_naive();
    match last_for_day {
        Some(day) if day >= today => None,
        _ => Some(today),
    }
}

pub(crate) fn day_start(day: NaiveDate) -> DateTime<Utc> {
    day.and_hms_opt(0, 0, 0)
        .map(|t| t.and_utc())
        .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap_or_default())
}
