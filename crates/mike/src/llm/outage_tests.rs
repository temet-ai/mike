use chrono::{Duration, TimeZone, Utc};

use super::*;

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 5, 8, 0, 0)
        .single()
        .unwrap_or_else(Utc::now)
}

#[test]
fn every_call_timing_out_fires_once_after_fifteen_minutes_then_once_on_recovery()
-> anyhow::Result<()> {
    let tracker = OutageTracker::default();
    let role = LlmRole::JudgeWorker;
    let mut fired = Vec::new();
    for minute in 0..=14 {
        let now = t0() + Duration::minutes(minute);
        fired.extend(tracker.record_failure(role, "LLM request timed out", now));
    }
    assert!(fired.is_empty(), "no notice before 15 minutes: {fired:?}");

    for minute in 15..=720 {
        let now = t0() + Duration::minutes(minute);
        fired.extend(tracker.record_failure(role, "LLM request timed out", now));
    }
    assert_eq!(fired.len(), 1, "one down notice for a 12 hour outage");
    assert_eq!(fired[0].kind, NoticeKind::Down);
    assert_eq!(fired[0].at, t0() + Duration::minutes(15));
    assert!(fired[0].message().contains("judge_worker"));
    assert!(fired[0].message().contains("timed out"));

    let back = tracker
        .record_success(role, t0() + Duration::minutes(721))
        .ok_or_else(|| anyhow::anyhow!("recovery must be announced"))?;
    assert_eq!(back.kind, NoticeKind::Recovered);
    assert_eq!(back.failing_since, t0());
    assert_eq!(
        tracker.record_success(role, t0() + Duration::minutes(722)),
        None
    );
    Ok(())
}

#[test]
fn a_success_inside_the_window_restarts_the_clock_and_says_nothing() {
    let tracker = OutageTracker::default();
    let role = LlmRole::Steerer;
    tracker.record_failure(role, "e", t0());
    assert_eq!(
        tracker.record_success(role, t0() + Duration::minutes(10)),
        None
    );
    assert_eq!(
        tracker.record_failure(role, "e", t0() + Duration::minutes(20)),
        None
    );
}

#[test]
fn roles_are_tracked_separately() {
    let tracker = OutageTracker::default();
    tracker.record_failure(LlmRole::Steerer, "e", t0());
    let other = tracker.record_failure(LlmRole::SessionJudge, "e", t0() + Duration::minutes(16));
    assert_eq!(other, None);
}
