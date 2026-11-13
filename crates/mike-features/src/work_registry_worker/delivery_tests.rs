use chrono::{Duration, Utc};

use super::MAX_DELIVERY_ATTEMPTS;
use super::delivery::{
    Decision, Retry, attention_message_uuid, current, decide, delivered, delivery_backoff,
    failed_once, next_retry, retry_state, wants_mail,
};
use crate::store::persistence::work_item_attention::AttentionRecord;
use crate::work_items::attention::Attention;

fn attention(kind: &'static str, subject: &str) -> Attention {
    Attention {
        kind,
        subject: subject.to_string(),
        since: Utc::now(),
        message: String::new(),
    }
}

fn recorded(
    delivered: bool,
    attempts: i32,
    last: Option<chrono::DateTime<Utc>>,
) -> AttentionRecord {
    AttentionRecord {
        id: 1,
        inserted: false,
        delivered,
        delivery_attempts: attempts,
        last_delivery_attempt_at: last,
        disposition: "pending".to_owned(),
    }
}

#[test]
fn a_session_is_mailed_only_about_what_still_holds_after_the_rescan() {
    let item = attention("stalled_wait", "job1@100");
    assert!(current(None, &item).is_none());
    assert!(current(Some(&[]), &item).is_none());
    assert!(current(Some(&[attention("stalled_wait", "job2@100")]), &item).is_none());
    assert!(current(Some(&[attention("result_lost", "job1@100")]), &item).is_none());
    assert!(current(Some(&[attention("stalled_wait", "job1@100")]), &item).is_some());
}

#[test]
fn a_main_idle_notice_is_withdrawn_when_main_resumed_or_the_open_count_changed() {
    let queued = attention("main_idle_with_open_work", "main@100#3a0j");
    let resumed = attention("main_idle_with_open_work", "main@900#3a0j");
    let fewer = attention("main_idle_with_open_work", "main@100#2a0j");
    assert!(current(Some(&[resumed]), &queued).is_none());
    assert!(current(Some(&[fewer]), &queued).is_none());
}

#[test]
fn delivery_uses_the_message_as_of_the_rescan() {
    let queued = attention("stalled_wait", "job1@100");
    let mut fresh = attention("stalled_wait", "job1@100");
    fresh.message = "as of delivery".to_owned();
    let found = current(Some(std::slice::from_ref(&fresh)), &queued);
    assert_eq!(found.map(|a| a.message.as_str()), Some("as of delivery"));
}

#[test]
fn an_item_recorded_in_log_mode_is_mailed_once_deliver_mode_is_on() {
    let item = attention("stalled_wait", "job1@100");
    let existing = recorded(false, 0, None);
    let now = Utc::now();
    assert!(
        !wants_mail(false, &existing, &item, now),
        "log mode never mails"
    );
    assert!(
        wants_mail(true, &existing, &item, now),
        "the row already exists and was never delivered, so deliver mode mails it"
    );
}

#[test]
fn a_delivered_or_stale_item_is_not_mailed_again() {
    let now = Utc::now();
    let item = attention("stalled_wait", "job1@100");
    assert!(!wants_mail(true, &recorded(true, 0, None), &item, now));
    let mut stale = attention("stalled_wait", "job1@100");
    stale.since = now - Duration::hours(5);
    assert!(!wants_mail(true, &recorded(false, 0, None), &stale, now));
}

#[test]
fn decide_old_pending_item_in_deliver_mode_settles_it_stale_once() {
    let now = Utc::now();
    let mut old = attention("stalled_wait", "job1@100");
    old.since = now - Duration::hours(5);
    let pending = recorded(false, 0, None);
    assert_eq!(decide(true, &old, &pending, now), Decision::SettleStale);
    let settled = AttentionRecord {
        disposition: "stale".to_owned(),
        ..pending.clone()
    };
    assert_eq!(decide(true, &old, &settled, now), Decision::Nothing);
    assert_eq!(decide(false, &old, &pending, now), Decision::Nothing);
    let fresh = attention("stalled_wait", "job1@100");
    assert_eq!(decide(true, &fresh, &pending, now), Decision::Mail);
    assert_eq!(
        decide(true, &fresh, &delivered(&pending), now),
        Decision::Nothing
    );
}

#[test]
fn a_failing_item_waits_out_its_backoff_then_gives_up() {
    let now = Utc::now();
    assert_eq!(retry_state(0, None, now), Retry::Due);
    let just_failed = Some(now - Duration::seconds(5));
    assert_eq!(retry_state(1, just_failed, now), Retry::Waiting);
    let long_ago = Some(now - Duration::hours(2));
    assert_eq!(retry_state(1, long_ago, now), Retry::Due);
    assert_eq!(
        retry_state(MAX_DELIVERY_ATTEMPTS, long_ago, now),
        Retry::Exhausted,
        "an item past its attempts is left for the operator, not retried"
    );
    let item = attention("stalled_wait", "job1@100");
    assert!(!wants_mail(
        true,
        &recorded(false, 1, just_failed),
        &item,
        now
    ));
    assert!(!wants_mail(
        true,
        &recorded(false, MAX_DELIVERY_ATTEMPTS, long_ago),
        &item,
        now
    ));
}

#[test]
fn failed_once_counts_the_attempt_and_gives_up_at_the_last_one() {
    let now = Utc::now();
    let once = failed_once(&recorded(false, 0, None), now);
    assert_eq!(once.delivery_attempts, 1);
    assert_eq!(once.last_delivery_attempt_at, Some(now));
    assert_eq!(once.disposition, "pending");
    let last = failed_once(&recorded(false, MAX_DELIVERY_ATTEMPTS - 1, None), now);
    assert_eq!(last.disposition, "given_up");
}

#[test]
fn next_retry_is_the_earliest_backoff_end_of_a_failing_item() {
    let now = Utc::now();
    let a = recorded(false, 1, Some(now));
    let b = recorded(false, 2, Some(now - Duration::seconds(30)));
    let done = recorded(true, 1, Some(now - Duration::hours(1)));
    let fresh = recorded(false, 0, None);
    assert_eq!(
        next_retry([&a, &b, &done, &fresh].into_iter()),
        Some(now + Duration::seconds(60))
    );
    assert_eq!(next_retry([&fresh].into_iter()), None);
}

#[test]
fn the_delivery_backoff_doubles_and_is_capped() {
    assert_eq!(delivery_backoff(1), Duration::seconds(60));
    assert_eq!(delivery_backoff(2), Duration::seconds(120));
    assert_eq!(delivery_backoff(3), Duration::seconds(240));
    assert_eq!(delivery_backoff(40), Duration::seconds(3600));
}

#[test]
fn the_mail_uuid_comes_from_the_attention_row_so_a_retry_cannot_duplicate_it() {
    assert_eq!(attention_message_uuid(7), attention_message_uuid(7));
    assert_ne!(attention_message_uuid(7), attention_message_uuid(8));
    assert_eq!(attention_message_uuid(7).get_version_num(), 8);
}
