use std::collections::BTreeMap;
use std::time::Duration;

use chrono::{TimeZone, Utc};
use uuid::Uuid;

use super::super::working::{Working, expiry};
use crate::change_feed::ChangeFeed;
use crate::change_record::WorkingCheckout;
use crate::store::models::RowChangedTable;

fn fed() -> ChangeFeed {
    let feed = ChangeFeed::new();
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

fn row(session: Uuid) -> WorkingCheckout {
    WorkingCheckout {
        checkout: "/work/alpha".to_owned(),
        project_id: 1,
        newest_request_at: Utc::now(),
        sessions: vec![session],
    }
}

fn settled(feed: &ChangeFeed, known: Uuid) -> Working {
    let mut working = Working::new(feed.clone());
    let stamp = working.before_read();
    working.read(stamp, &[row(known)], None);
    working.due(Utc::now());
    working
}

#[tokio::test]
async fn working_before_its_first_read_is_due() {
    let mut working = Working::new(fed());
    assert!(working.due(Utc::now()));
}

#[tokio::test]
async fn working_after_a_request_of_a_known_session_is_not_due() {
    let feed = fed();
    let known = Uuid::new_v4();
    let mut working = settled(&feed, known);
    assert!(!working.due(Utc::now()));
    feed.record(RowChangedTable::Requests, Some(&known.to_string()));
    feed.record(RowChangedTable::Sessions, Some(&known.to_string()));
    assert!(!working.due(Utc::now()));
}

#[tokio::test]
async fn working_after_a_request_of_an_unknown_session_is_due_once() {
    let feed = fed();
    let mut working = settled(&feed, Uuid::new_v4());
    feed.record(RowChangedTable::Requests, Some(&Uuid::new_v4().to_string()));
    assert!(working.due(Utc::now()));
    assert!(!working.due(Utc::now()), "the notice was drained");
}

#[tokio::test]
async fn working_after_a_location_or_edit_notice_is_due_until_read_again() {
    for table in [RowChangedTable::Locations, RowChangedTable::CheckoutEdits] {
        let feed = fed();
        let known = Uuid::new_v4();
        let mut working = settled(&feed, known);
        feed.record(table, None);
        assert!(working.due(Utc::now()), "{table:?}");
        assert!(working.due(Utc::now()), "{table:?} stays due until a read");
        let stamp = working.before_read();
        working.read(stamp, &[row(known)], None);
        assert!(!working.due(Utc::now()), "{table:?}");
    }
}

#[tokio::test]
async fn working_after_a_session_projects_generation_is_due_and_after_others_is_not() {
    let feed = fed();
    let mut working = settled(&feed, Uuid::new_v4());
    feed.record(RowChangedTable::ChangeGenerations, Some("lessons"));
    assert!(!working.due(Utc::now()));
    feed.record(RowChangedTable::ChangeGenerations, Some("session_projects"));
    assert!(working.due(Utc::now()));
}

#[tokio::test]
async fn working_after_a_resync_is_due() {
    let feed = fed();
    let mut working = settled(&feed, Uuid::new_v4());
    feed.resync();
    assert!(working.due(Utc::now()));
}

#[tokio::test]
async fn working_once_its_window_runs_out_is_due() {
    let feed = fed();
    let known = Uuid::new_v4();
    let mut working = Working::new(feed.clone());
    let now = Utc::now();
    let stamp = working.before_read();
    working.read(
        stamp,
        &[row(known)],
        Some(now + chrono::Duration::minutes(5)),
    );
    working.due(now);
    assert!(!working.due(now + chrono::Duration::minutes(4)));
    assert!(working.due(now + chrono::Duration::minutes(5)));
    assert_eq!(
        working.wait(now, Duration::from_secs(30)),
        Duration::from_secs(30)
    );
    assert_eq!(
        working.wait(now + chrono::Duration::minutes(10), Duration::from_secs(30)),
        Duration::ZERO
    );
}

#[tokio::test]
async fn working_without_a_fed_feed_is_always_due() {
    let feed = ChangeFeed::new();
    let mut working = settled(&feed, Uuid::new_v4());
    assert!(working.due(Utc::now()));
}

#[test]
fn expiry_of_watched_roots_is_the_oldest_newest_plus_the_window() -> anyhow::Result<()> {
    let older = Utc
        .with_ymd_and_hms(2026, 11, 23, 9, 0, 0)
        .single()
        .ok_or_else(|| anyhow::anyhow!("valid time"))?;
    let newer = older + chrono::Duration::minutes(10);
    let newest = BTreeMap::from([("/a".to_owned(), newer), ("/b".to_owned(), older)]);
    assert_eq!(
        expiry(&newest),
        Some(older + chrono::Duration::minutes(30) + chrono::Duration::seconds(1))
    );
    assert_eq!(expiry(&BTreeMap::new()), None);
    Ok(())
}
