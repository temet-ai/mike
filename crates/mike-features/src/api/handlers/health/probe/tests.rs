use super::*;
use std::future::{pending, ready};

#[tokio::test]
async fn bounded_reports_the_value_when_work_finishes_first() {
    let outcome = bounded(ready(7), pending::<()>()).await;
    assert!(matches!(outcome, Bounded::Done(7)));
}

#[tokio::test]
async fn bounded_reports_timeout_when_the_deadline_fires_first() {
    let outcome = bounded(pending::<u8>(), ready(())).await;
    assert!(matches!(outcome, Bounded::TimedOut));
}

#[tokio::test]
async fn bounded_prefers_finished_work_when_both_are_ready() {
    let outcome = bounded(ready(1), ready(())).await;
    assert!(matches!(outcome, Bounded::Done(1)));
}

#[test]
fn classify_keeps_timeout_apart_from_failure() {
    let ok: Bounded<Result<u8, &str>> = Bounded::Done(Ok(3));
    let failed: Bounded<Result<u8, &str>> = Bounded::Done(Err("down"));
    let late: Bounded<Result<u8, &str>> = Bounded::TimedOut;
    assert_eq!(classify(ok), (ProbeState::Ok, Some(3)));
    assert_eq!(classify(failed), (ProbeState::Failed, None));
    assert_eq!(classify(late), (ProbeState::TimedOut, None));
}

#[tokio::test]
async fn persistence_probe_without_a_handle_is_not_configured() {
    let state = probe_persistence(None, LIVE_PROBE_DEADLINE).await;
    assert_eq!(state, ProbeState::NotConfigured);
}

#[tokio::test]
async fn latest_capture_probe_without_a_handle_is_not_configured() {
    let (state, newest) = probe_latest_capture(None, LIVE_PROBE_DEADLINE).await;
    assert_eq!(state, ProbeState::NotConfigured);
    assert!(newest.is_none());
}

#[test]
fn live_deadline_is_well_under_a_second() {
    assert!(LIVE_PROBE_DEADLINE < Duration::from_millis(500));
}

#[test]
fn probe_states_serialize_as_snake_case_words() -> anyhow::Result<()> {
    assert_eq!(
        serde_json::to_string(&ProbeState::TimedOut)?,
        "\"timed_out\""
    );
    assert_eq!(
        serde_json::to_string(&ProbeState::NotConfigured)?,
        "\"not_configured\""
    );
    Ok(())
}
