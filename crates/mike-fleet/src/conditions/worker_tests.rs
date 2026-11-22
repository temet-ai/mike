use std::time::Duration;

use chrono::Utc;

use super::worker::until;

#[test]
fn until_with_nothing_armed_waits_for_a_registration() {
    assert_eq!(until(None, Utc::now()), Duration::from_secs(600));
}

#[test]
fn until_a_future_condition_wakes_when_it_starts() {
    let now = Utc::now();
    let at = now + chrono::Duration::seconds(90);
    assert_eq!(until(Some(at), now), Duration::from_secs(90));
}

#[test]
fn until_a_far_condition_is_capped_by_the_backstop() {
    let now = Utc::now();
    let at = now + chrono::Duration::hours(5);
    assert_eq!(until(Some(at), now), Duration::from_secs(600));
}

#[test]
fn until_a_past_time_looks_again_at_once() {
    let now = Utc::now();
    assert_eq!(
        until(Some(now - chrono::Duration::seconds(5)), now),
        Duration::ZERO
    );
}
