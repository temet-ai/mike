use super::live_backlog::LiveQueueReading;
use chrono::{DateTime, Utc};

fn at(secs: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(1_700_000_000 + secs, 0).unwrap_or_default()
}

#[test]
fn reading_empty_claim_has_no_stale_backlog() {
    let reading = LiveQueueReading::from_claim(&[], 10, at(0));
    let stale = reading.stale(at(0));
    assert_eq!(stale.count, 0);
    assert_eq!(stale.oldest_closed_at, None);
    assert!(!stale.at_least);
    assert!(!reading.saturated);
}

#[test]
fn reading_counts_only_turns_closed_before_the_cutoff() {
    let claimed = [(3, at(-100)), (1, at(-500)), (2, at(-10))];
    let reading = LiveQueueReading::from_claim(&claimed, 10, at(0));
    let stale = reading.stale(at(-50));
    assert_eq!(stale.count, 2);
    assert_eq!(stale.oldest_closed_at, Some(at(-500)));
    assert!(!stale.at_least);
}

#[test]
fn reading_saturated_claim_of_only_stale_turns_is_a_lower_bound() {
    let claimed = [(1, at(-500)), (2, at(-400))];
    let reading = LiveQueueReading::from_claim(&claimed, 2, at(0));
    let stale = reading.stale(at(-50));
    assert_eq!(stale.count, 2);
    assert!(stale.at_least);
}

#[test]
fn reading_saturated_claim_ending_in_fresh_turns_is_exact() {
    let claimed = [(1, at(-500)), (2, at(-10))];
    let reading = LiveQueueReading::from_claim(&claimed, 2, at(0));
    let stale = reading.stale(at(-50));
    assert_eq!(stale.count, 1);
    assert!(!stale.at_least);
}

#[test]
fn reading_age_is_measured_from_the_claim() {
    let reading = LiveQueueReading::from_claim(&[], 5, at(0));
    assert_eq!(reading.age_secs(at(42)), 42);
    assert_eq!(reading.age_secs(at(-5)), 0);
}
