use super::*;
use crate::judge_worker::live_backlog::{self, LiveQueueReading};

pub(crate) const UNJUDGED_TURN_ALARM_AGE_SECS: i64 = 6 * 60 * 60;

pub(crate) fn judge_health(state: &AppState) -> JudgeHealth {
    judge_health_from(
        judge_expected(state),
        live_backlog::current().as_deref(),
        chrono::Utc::now(),
    )
}

pub(super) fn judge_expected(state: &AppState) -> bool {
    state.feature_toggle_overlay.load().judge_worker
}

pub(crate) fn judge_health_from(
    expected: bool,
    reading: Option<&LiveQueueReading>,
    now: chrono::DateTime<chrono::Utc>,
) -> JudgeHealth {
    let reading = reading.filter(|_| expected);
    let closed_before = now - chrono::Duration::seconds(UNJUDGED_TURN_ALARM_AGE_SECS);
    let stale = reading.map(|reading| reading.stale(closed_before));
    let stale_turns = stale.map(|stale| stale.count);
    JudgeHealth {
        judge_expected: expected,
        stale_after_secs: UNJUDGED_TURN_ALARM_AGE_SECS,
        stale_unjudged_closed_turns: stale_turns,
        stale_unjudged_at_least: stale.is_some_and(|stale| stale.at_least),
        oldest_unjudged_closed_at: stale
            .and_then(|stale| stale.oldest_closed_at)
            .map(|closed_at| closed_at.to_rfc3339()),
        backlog_alarm: backlog_alarm(expected, stale_turns),
        backlog_age_secs: reading.map(|reading| reading.age_secs(now)),
    }
}

pub(crate) fn backlog_alarm(expected: bool, stale_turns: Option<i64>) -> bool {
    expected && stale_turns.is_some_and(|count| count > 0)
}

pub(crate) fn apply_judge_alarm(status: &'static str, alarm: bool) -> &'static str {
    if alarm { "degraded" } else { status }
}

#[cfg(test)]
mod tests;
