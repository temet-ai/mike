use chrono::{Duration, Utc};
use uuid::Uuid;

use super::{silent, wake_key, wake_reminder};
use crate::store::models::ReminderSource;

#[test]
fn a_seat_with_no_request_is_silent() {
    assert!(silent(None, Utc::now()));
}

#[test]
fn a_seat_that_spoke_under_five_minutes_ago_is_not_woken() {
    let now = Utc::now();
    assert!(!silent(Some(now - Duration::seconds(299)), now));
}

#[test]
fn a_seat_silent_for_five_minutes_is_woken() {
    let now = Utc::now();
    assert!(silent(Some(now - Duration::seconds(300)), now));
}

#[test]
fn the_wake_key_changes_when_the_seat_speaks_again() {
    let session = Uuid::nil();
    let first = Utc::now();
    let second = first + Duration::minutes(10);
    assert_ne!(
        wake_key(session, Some(first)),
        wake_key(session, Some(second))
    );
}

#[test]
fn the_wake_key_is_stable_for_one_silent_stretch() {
    let session = Uuid::nil();
    let last = Utc::now();
    assert_eq!(wake_key(session, Some(last)), wake_key(session, Some(last)));
}

#[test]
fn the_wake_names_the_unread_count_and_is_a_mailbox_source() {
    let reminder = wake_reminder(3);
    assert!(reminder.payload.contains("3 unread"));
    assert!(matches!(
        reminder.source,
        ReminderSource::SessionMailbox { .. }
    ));
}
