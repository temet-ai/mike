use super::*;

fn at(secs: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(1_700_000_000 + secs, 0).unwrap_or_default()
}

fn session(
    id: &str,
    first_seen: i64,
    last_work: Option<i64>,
    last_sidecar: Option<i64>,
) -> SessionActivity {
    SessionActivity {
        session_id: id.to_owned(),
        first_seen_at: at(first_seen),
        last_work_at: last_work.map(at),
        last_sidecar_at: last_sidecar.map(at),
    }
}

#[test]
fn sidecar_continues_while_work_stopped_is_a_gap() {
    let now = at(1000);
    let stalled = session("stalled", 0, Some(100), Some(990));
    let gaps = find_gaps(&[stalled], now);
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0].session_id, "stalled");
    assert_eq!(gaps[0].work_silent_secs, 900);
    assert_eq!(gaps[0].last_work_at, Some(at(100).to_rfc3339()));
}

#[test]
fn healthy_session_next_to_a_stalled_one_is_not_flagged() {
    let now = at(1000);
    let healthy = session("healthy", 0, Some(990), Some(995));
    let stalled = session("stalled", 0, Some(100), Some(990));
    let gaps = find_gaps(&[healthy, stalled], now);
    assert_eq!(
        gaps.iter()
            .map(|g| g.session_id.as_str())
            .collect::<Vec<_>>(),
        vec!["stalled"]
    );
}

#[test]
fn idle_installation_with_no_recent_sidecar_requests_is_not_a_gap() {
    let now = at(10_000);
    let idle = session("idle", 0, Some(100), Some(200));
    assert!(find_gaps(&[idle], now).is_empty());
}

#[test]
fn empty_installation_is_not_a_gap() {
    assert!(find_gaps(&[], at(0)).is_empty());
}

#[test]
fn work_silence_exactly_at_the_threshold_is_not_a_gap_one_second_more_is() {
    let sidecar = 1000;
    let now = at(sidecar + 1);
    let at_threshold = session("a", 0, Some(1001 - CAPTURE_GAP_SILENCE_SECS), Some(sidecar));
    let past_threshold = session(
        "b",
        0,
        Some(1001 - CAPTURE_GAP_SILENCE_SECS - 1),
        Some(sidecar),
    );
    assert!(find_gaps(&[at_threshold], now).is_empty());
    assert_eq!(find_gaps(&[past_threshold], now).len(), 1);
}

#[test]
fn sidecar_age_exactly_at_the_recent_window_counts_one_second_more_does_not() {
    let now = at(5000);
    let inside = session(
        "in",
        0,
        Some(100),
        Some(5000 - CAPTURE_GAP_SIDECAR_RECENT_SECS),
    );
    let outside = session(
        "out",
        0,
        Some(100),
        Some(5000 - CAPTURE_GAP_SIDECAR_RECENT_SECS - 1),
    );
    assert_eq!(find_gaps(&[inside], now).len(), 1);
    assert!(find_gaps(&[outside], now).is_empty());
}

#[test]
fn session_with_sidecar_only_is_a_gap_once_seen_longer_than_the_threshold() {
    let now = at(1000);
    let long_seen = session("long", 100, None, Some(990));
    let just_started = session("new", 900, None, Some(990));
    let gaps = find_gaps(&[long_seen, just_started], now);
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0].session_id, "long");
    assert_eq!(gaps[0].last_work_at, None);
}

#[test]
fn session_without_any_sidecar_request_is_never_a_gap() {
    let now = at(5000);
    let quiet = session("quiet", 0, Some(100), None);
    assert!(find_gaps(&[quiet], now).is_empty());
}

#[test]
fn gaps_are_ordered_by_session_id() {
    let now = at(1000);
    let b = session("b", 0, Some(100), Some(990));
    let a = session("a", 0, Some(100), Some(990));
    let ids: Vec<String> = find_gaps(&[b, a], now)
        .into_iter()
        .map(|g| g.session_id)
        .collect();
    assert_eq!(ids, vec!["a".to_owned(), "b".to_owned()]);
}

#[test]
fn health_carries_the_thresholds_and_raises_the_alarm_for_a_gap() {
    let now = at(1000);
    let reading = Stored {
        value: vec![session("stalled", 0, Some(100), Some(990))],
        refreshed_at: at(970),
    };
    let health = capture_gap_health(Some(&reading), now);
    assert!(health.alarm);
    assert_eq!(health.silence_threshold_secs, CAPTURE_GAP_SILENCE_SECS);
    assert_eq!(health.sidecar_recent_secs, CAPTURE_GAP_SIDECAR_RECENT_SECS);
    assert_eq!(health.reading_age_secs, Some(30));
    assert_eq!(health.sessions.len(), 1);
}

#[test]
fn health_without_a_reading_is_quiet_and_says_it_has_no_reading() {
    let health = capture_gap_health(None, at(1000));
    assert!(!health.alarm);
    assert!(health.sessions.is_empty());
    assert_eq!(health.reading_age_secs, None);
}

#[test]
fn alarm_degrades_an_ok_status_and_no_alarm_leaves_it() {
    assert_eq!(apply_gap_alarm("ok", true), "degraded");
    assert_eq!(apply_gap_alarm("ok", false), "ok");
    assert_eq!(apply_gap_alarm("degraded", false), "degraded");
}

#[test]
fn a_stale_reading_stops_alarming_once_its_sidecar_requests_are_no_longer_recent() {
    let reading = Stored {
        value: vec![session("stalled", 0, Some(100), Some(990))],
        refreshed_at: at(1000),
    };
    assert!(capture_gap_health(Some(&reading), at(1000)).alarm);
    let later = at(990 + CAPTURE_GAP_SIDECAR_RECENT_SECS + 1);
    assert!(!capture_gap_health(Some(&reading), later).alarm);
}
