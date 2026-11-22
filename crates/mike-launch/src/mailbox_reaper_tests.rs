use std::time::Duration;

use chrono::Utc;

use super::{MAIL, MailDue, NOTICE_DELAY};
use crate::change_feed::ChangeFeed;
use crate::store::models::RowChangedTable;

fn fed_feed() -> ChangeFeed {
    let feed = ChangeFeed::default();
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    crate::change_feed::spawn_change_feed(
        &feed,
        None,
        rx,
        tokio_util::sync::CancellationToken::new(),
        &tokio_util::task::TaskTracker::new(),
    );
    feed
}

fn swept(feed: &ChangeFeed, earliest: Option<chrono::DateTime<Utc>>) -> MailDue {
    let mut due = MailDue::default();
    due.checked(feed);
    due.found(earliest);
    due
}

#[tokio::test]
async fn reaper_sweeps_on_its_first_tick() {
    assert!(MailDue::default().is_due(&fed_feed(), Utc::now()));
}

#[test]
fn reaper_without_a_change_feed_sweeps_every_tick() {
    let feed = ChangeFeed::default();
    assert!(swept(&feed, None).is_due(&feed, Utc::now()));
}

#[tokio::test]
async fn reaper_with_no_undelivered_mail_and_no_change_skips() {
    let feed = fed_feed();
    let due = swept(&feed, None);
    assert!(!due.is_due(&feed, Utc::now() + chrono::Duration::hours(24)));
}

#[tokio::test]
async fn reaper_waits_until_the_earliest_undelivered_mail_is_due() {
    let feed = fed_feed();
    let now = Utc::now();
    let due = swept(&feed, Some(now));
    assert!(!due.is_due(&feed, now + NOTICE_DELAY - chrono::Duration::seconds(1)));
    assert!(due.is_due(&feed, now + NOTICE_DELAY));
}

#[tokio::test]
async fn reaper_sweeps_once_mail_that_changed_could_be_due() {
    let feed = fed_feed();
    let mut due = swept(&feed, None);
    feed.record(RowChangedTable::MailboxMessages, Some("recipient"));
    assert!(
        tokio::time::timeout(Duration::from_secs(1), due.next_change(&feed))
            .await
            .is_ok(),
        "the mailbox change ends the wait"
    );
    let changed = Utc::now();
    assert!(!due.is_due(&feed, changed + NOTICE_DELAY - chrono::Duration::seconds(5)));
    assert!(due.is_due(&feed, changed + NOTICE_DELAY + chrono::Duration::seconds(1)));
}

#[tokio::test]
async fn reaper_ignores_changes_to_other_tables() {
    let feed = fed_feed();
    let mut due = swept(&feed, None);
    feed.record(RowChangedTable::Requests, Some("recipient"));
    feed.record(RowChangedTable::Sessions, Some("recipient"));
    assert!(
        tokio::time::timeout(Duration::from_millis(300), due.next_change(&feed))
            .await
            .is_err()
    );
    assert!(!feed.changed_since(&MAIL, due.since));
    assert!(!due.is_due(&feed, Utc::now() + chrono::Duration::hours(1)));
}
