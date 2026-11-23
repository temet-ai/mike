use std::time::Duration;

use chrono::Utc;

use super::{IDLE_TURN_THRESHOLD, REAP_BACKSTOP, is_stale, wait_until_stale};

fn minutes(n: i64) -> chrono::Duration {
    chrono::Duration::minutes(n)
}

#[test]
fn wait_with_no_open_turn_is_the_backstop() {
    assert_eq!(
        wait_until_stale(None, IDLE_TURN_THRESHOLD, Utc::now()),
        REAP_BACKSTOP
    );
}

#[test]
fn wait_for_a_turn_seen_just_now_is_capped_at_the_backstop() {
    let now = Utc::now();
    assert_eq!(
        wait_until_stale(Some(now), IDLE_TURN_THRESHOLD, now),
        REAP_BACKSTOP
    );
}

#[test]
fn wait_for_a_turn_near_the_threshold_ends_just_after_it_goes_stale() {
    let now = Utc::now();
    let seen = now - minutes(27);
    let wait = wait_until_stale(Some(seen), IDLE_TURN_THRESHOLD, now);
    assert_eq!(wait, Duration::from_secs(3 * 60 + 1));
    let woken = now + chrono::Duration::from_std(wait).unwrap_or_default();
    assert!(is_stale(seen, IDLE_TURN_THRESHOLD, woken));
    assert!(!is_stale(seen, IDLE_TURN_THRESHOLD, now));
}

#[test]
fn wait_for_a_turn_already_stale_is_immediate() {
    let now = Utc::now();
    let wait = wait_until_stale(Some(now - minutes(45)), IDLE_TURN_THRESHOLD, now);
    assert!(wait <= Duration::from_secs(1));
}

#[test]
fn stale_needs_the_session_quiet_for_longer_than_the_threshold() {
    let now = Utc::now();
    assert!(!is_stale(now - minutes(30), IDLE_TURN_THRESHOLD, now));
    assert!(is_stale(
        now - minutes(30) - chrono::Duration::milliseconds(1),
        IDLE_TURN_THRESHOLD,
        now
    ));
}
