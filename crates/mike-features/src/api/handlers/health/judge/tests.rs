use super::*;

fn now() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap_or_default()
}

fn hours_ago(hours: i64) -> chrono::DateTime<chrono::Utc> {
    now() - chrono::Duration::hours(hours)
}

fn reading(claimed: &[(i64, chrono::DateTime<chrono::Utc>)], limit: i64) -> LiveQueueReading {
    LiveQueueReading::from_claim(claimed, limit, now() - chrono::Duration::seconds(30))
}

#[test]
fn judge_alarm_raised_when_the_last_claim_found_turns_owed_past_the_threshold() {
    let reading = reading(&[(1, hours_ago(10)), (2, hours_ago(1))], 10);

    let health = judge_health_from(true, Some(&reading), now());

    assert!(health.judge_expected);
    assert_eq!(health.stale_unjudged_closed_turns, Some(1));
    assert_eq!(
        health.oldest_unjudged_closed_at,
        Some(hours_ago(10).to_rfc3339())
    );
    assert!(!health.stale_unjudged_at_least);
    assert!(health.backlog_alarm);
    assert_eq!(health.backlog_age_secs, Some(30));
}

#[test]
fn judge_alarm_quiet_when_the_last_claim_found_only_fresh_turns() {
    let reading = reading(&[(1, hours_ago(1))], 10);

    let health = judge_health_from(true, Some(&reading), now());

    assert_eq!(health.stale_unjudged_closed_turns, Some(0));
    assert_eq!(health.oldest_unjudged_closed_at, None);
    assert!(!health.backlog_alarm);
}

#[test]
fn judge_alarm_quiet_when_the_last_claim_found_nothing_owed() {
    let reading = reading(&[], 10);

    let health = judge_health_from(true, Some(&reading), now());

    assert_eq!(health.stale_unjudged_closed_turns, Some(0));
    assert!(!health.backlog_alarm);
}

#[test]
fn judge_stale_count_is_a_lower_bound_when_a_full_claim_was_all_stale() {
    let reading = reading(&[(1, hours_ago(12)), (2, hours_ago(10))], 2);

    let health = judge_health_from(true, Some(&reading), now());

    assert_eq!(health.stale_unjudged_closed_turns, Some(2));
    assert!(health.stale_unjudged_at_least);
    assert!(health.backlog_alarm);
}

#[test]
fn judge_alarm_quiet_when_the_judge_is_not_expected_to_run() {
    let reading = reading(&[(1, hours_ago(10))], 10);

    let health = judge_health_from(false, Some(&reading), now());

    assert!(!health.judge_expected);
    assert_eq!(health.stale_unjudged_closed_turns, None);
    assert_eq!(health.backlog_age_secs, None);
    assert!(!health.backlog_alarm);
}

#[test]
fn judge_health_without_a_published_claim_reports_no_backlog() {
    let health = judge_health_from(true, None, now());

    assert_eq!(health.stale_unjudged_closed_turns, None);
    assert_eq!(health.backlog_age_secs, None);
    assert!(!health.backlog_alarm);
}

#[test]
fn backlog_alarm_requires_an_expected_judge_and_a_positive_count() {
    assert!(backlog_alarm(true, Some(1)));
    assert!(backlog_alarm(true, Some(56)));
    assert!(!backlog_alarm(true, Some(0)));
    assert!(!backlog_alarm(true, None));
    assert!(!backlog_alarm(false, Some(56)));
}

#[test]
fn judge_alarm_degrades_an_ok_status_and_leaves_degraded_alone() {
    assert_eq!(apply_judge_alarm("ok", true), "degraded");
    assert_eq!(apply_judge_alarm("ok", false), "ok");
    assert_eq!(apply_judge_alarm("degraded", false), "degraded");
    assert_eq!(apply_judge_alarm("degraded", true), "degraded");
}
