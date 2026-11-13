use chrono::{DateTime, Utc};

use crate::workspace::Liveness;

pub const RECENCY_WINDOW_SECS: i64 = 90;

pub fn derive_state(
    now: DateTime<Utc>,
    busy: bool,
    last_activity_at: Option<DateTime<Utc>>,
    registration: Option<Liveness>,
    last_status: Option<&str>,
) -> &'static str {
    let recently_active = last_activity_at.is_some_and(|last| {
        let elapsed_secs = (now - last).num_seconds();
        (0..RECENCY_WINDOW_SECS).contains(&elapsed_secs)
    });
    if busy || recently_active {
        return "working";
    }
    if registration == Some(Liveness::Ended) {
        return "done";
    }
    if last_status.is_some_and(|s| s.starts_with("Error")) {
        return "error";
    }
    if registration == Some(Liveness::Uncaptured) {
        return "unknown";
    }
    if registration == Some(Liveness::Live) {
        return "idle";
    }
    if registration == Some(Liveness::Stale) || last_status.is_some_and(|s| !s.trim().is_empty()) {
        return "stale";
    }
    "unknown"
}

pub fn derive_state_detail(state: &str, last_status: Option<&str>) -> Option<String> {
    if state != "error" {
        return None;
    }
    last_status
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
}

pub fn derive_error_kind(state: &str, last_status: Option<&str>) -> Option<&'static str> {
    if state != "error" {
        return None;
    }
    match last_status {
        Some("Error: proxy_crashed_during_stream") => Some("infra"),
        Some("Error: stream aborted") => Some("ended_early"),
        Some(s) if s.starts_with("Error") => Some("agent"),
        _ => None,
    }
}

pub struct DerivedQuality {
    pub outcome: String,
    pub rationale: String,
}

pub fn derive_quality(outcome: Option<&str>, rationale: Option<&str>) -> Option<DerivedQuality> {
    let outcome = outcome?.trim();
    if outcome.is_empty() || outcome == "unknown" {
        return None;
    }
    Some(DerivedQuality {
        outcome: outcome.to_string(),
        rationale: rationale
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or_default()
            .to_string(),
    })
}

pub fn derive_role(declared_role: Option<&str>) -> Option<String> {
    declared_role
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn t(secs_ago: i64) -> (DateTime<Utc>, Option<DateTime<Utc>>) {
        let now = Utc::now();
        (now, Some(now - Duration::seconds(secs_ago)))
    }

    #[test]
    fn derive_state_no_last_activity_at_all_is_never_working_from_recency() {
        let now = Utc::now();
        assert_ne!(
            derive_state(now, false, None, Some(Liveness::Live), Some("Complete")),
            "working"
        );
    }

    #[test]
    fn derive_state_busy_always_wins() {
        let (now, last_activity_at) = t(10_000);
        assert_eq!(
            derive_state(
                now,
                true,
                last_activity_at,
                Some(Liveness::Ended),
                Some("Complete")
            ),
            "working"
        );
    }

    #[test]
    fn derive_state_recent_activity_reads_working_even_when_not_busy() {
        let (now, last_activity_at) = t(30);
        assert_eq!(
            derive_state(now, false, last_activity_at, None, Some("Complete")),
            "working"
        );
    }

    #[test]
    fn derive_state_activity_just_outside_window_is_not_working() {
        let (now, last_activity_at) = t(RECENCY_WINDOW_SECS + 1);
        assert_ne!(
            derive_state(
                now,
                false,
                last_activity_at,
                Some(Liveness::Live),
                Some("Complete")
            ),
            "working"
        );
    }

    #[test]
    fn derive_state_complete_status_alone_never_reads_done() {
        let (now, last_activity_at) = t(10_000);
        assert_ne!(
            derive_state(
                now,
                false,
                last_activity_at,
                Some(Liveness::Live),
                Some("Complete")
            ),
            "done"
        );
        assert_ne!(
            derive_state(
                now,
                false,
                last_activity_at,
                Some(Liveness::Stale),
                Some("Complete")
            ),
            "done"
        );
        assert_ne!(
            derive_state(now, false, last_activity_at, None, Some("Complete")),
            "done"
        );
    }

    #[test]
    fn derive_state_done_requires_explicit_registration_end() {
        let (now, last_activity_at) = t(10_000);
        assert_eq!(
            derive_state(
                now,
                false,
                last_activity_at,
                Some(Liveness::Ended),
                Some("Complete")
            ),
            "done"
        );
    }

    #[test]
    fn derive_state_ended_registration_wins_over_error_status() {
        let (now, last_activity_at) = t(10_000);
        assert_eq!(
            derive_state(
                now,
                false,
                last_activity_at,
                Some(Liveness::Ended),
                Some("Error: rate limited")
            ),
            "done"
        );
    }

    #[test]
    fn derive_state_live_lease_is_idle_between_turns() {
        let (now, last_activity_at) = t(10_000);
        assert_eq!(
            derive_state(
                now,
                false,
                last_activity_at,
                Some(Liveness::Live),
                Some("Complete")
            ),
            "idle"
        );
    }

    #[test]
    fn derive_state_lapsed_lease_is_stale_not_dead() {
        let (now, last_activity_at) = t(10_000);
        assert_eq!(
            derive_state(
                now,
                false,
                last_activity_at,
                Some(Liveness::Stale),
                Some("Complete")
            ),
            "stale"
        );
    }

    #[test]
    fn derive_state_no_registration_but_some_history_is_stale() {
        let (now, last_activity_at) = t(10_000);
        assert_eq!(
            derive_state(now, false, last_activity_at, None, Some("Complete")),
            "stale"
        );
    }

    #[test]
    fn derive_state_error_status_wins_over_idle_and_stale() {
        let (now, last_activity_at) = t(10_000);
        assert_eq!(
            derive_state(
                now,
                false,
                last_activity_at,
                Some(Liveness::Live),
                Some("Error: stream aborted")
            ),
            "error"
        );
        assert_eq!(
            derive_state(
                now,
                false,
                last_activity_at,
                Some(Liveness::Stale),
                Some("Error: proxy_crashed_during_stream")
            ),
            "error"
        );
        assert_eq!(
            derive_state(
                now,
                false,
                last_activity_at,
                None,
                Some("Error: 500 upstream")
            ),
            "error"
        );
    }

    #[test]
    fn derive_state_no_signal_at_all_is_unknown() {
        let (now, last_activity_at) = t(10_000);
        assert_eq!(
            derive_state(now, false, last_activity_at, None, None),
            "unknown"
        );
        assert_eq!(
            derive_state(now, false, last_activity_at, None, Some("")),
            "unknown"
        );
    }

    #[test]
    fn derive_state_detail_carries_the_verbatim_error_string() {
        assert_eq!(
            derive_state_detail("error", Some("Error: rate limited")),
            Some("Error: rate limited".to_string())
        );
    }

    #[test]
    fn derive_state_detail_none_when_state_is_not_error() {
        assert_eq!(derive_state_detail("working", Some("Error: x")), None);
        assert_eq!(derive_state_detail("done", Some("Complete")), None);
        assert_eq!(derive_state_detail("idle", Some("Complete")), None);
        assert_eq!(derive_state_detail("stale", None), None);
    }

    #[test]
    fn derive_error_kind_proxy_crash_is_infra() {
        assert_eq!(
            derive_error_kind("error", Some("Error: proxy_crashed_during_stream")),
            Some("infra")
        );
    }

    #[test]
    fn derive_error_kind_stream_aborted_is_ended_early() {
        assert_eq!(
            derive_error_kind("error", Some("Error: stream aborted")),
            Some("ended_early")
        );
    }

    #[test]
    fn derive_error_kind_other_error_text_is_agent() {
        assert_eq!(
            derive_error_kind("error", Some("Error: rate limited")),
            Some("agent")
        );
    }

    #[test]
    fn derive_error_kind_none_when_state_is_not_error() {
        assert_eq!(
            derive_error_kind("idle", Some("Error: proxy_crashed_during_stream")),
            None
        );
        assert_eq!(derive_error_kind("done", None), None);
    }

    #[test]
    fn derive_error_kind_never_consults_judge_outcome() {
        assert_eq!(derive_error_kind("done", None), None);
    }

    #[test]
    fn derive_quality_carries_outcome_and_rationale() {
        let Some(q) = derive_quality(Some("failure"), Some("gave up early")) else {
            panic!("expected Some(quality)");
        };
        assert_eq!(q.outcome, "failure");
        assert_eq!(q.rationale, "gave up early");
    }

    #[test]
    fn derive_quality_missing_rationale_is_empty_not_none() {
        let Some(q) = derive_quality(Some("success"), None) else {
            panic!("expected Some(quality)");
        };
        assert_eq!(q.outcome, "success");
        assert_eq!(q.rationale, "");
    }

    #[test]
    fn derive_quality_none_when_no_verdict_yet() {
        assert!(derive_quality(None, None).is_none());
    }

    #[test]
    fn derive_quality_none_when_outcome_is_the_unknown_placeholder() {
        assert!(derive_quality(Some("unknown"), Some("n/a")).is_none());
    }

    #[test]
    fn derive_role_returns_the_declared_role() {
        assert_eq!(
            derive_role(Some("coordinator")),
            Some("coordinator".to_string())
        );
    }

    #[test]
    fn derive_role_blank_declared_role_is_none() {
        assert_eq!(derive_role(Some("  ")), None);
        assert_eq!(derive_role(Some("")), None);
    }

    #[test]
    fn derive_role_none_when_undeclared() {
        assert_eq!(derive_role(None), None);
    }
}
