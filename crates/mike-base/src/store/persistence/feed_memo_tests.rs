use uuid::Uuid;

use super::{FeedMemos, Memo, Scope};
use crate::change_feed::ChangeFeed;
use crate::store::models::RowChangedTable;

fn fleet_memo() -> Memo<Option<u8>> {
    Memo::new(Scope {
        tables: &[RowChangedTable::LaunchFleets],
        per_session: &[RowChangedTable::LaunchLaunches, RowChangedTable::Sessions],
    })
}

fn attached() -> (FeedMemos, ChangeFeed) {
    let memos = FeedMemos::default();
    let feed = ChangeFeed::new();
    let _ = memos.inner.feed.set(feed.clone());
    (memos, feed)
}

#[test]
fn briefs_without_an_attached_feed_are_never_memoised() {
    let memos = FeedMemos::default();
    assert!(memos.briefs().is_none());
    assert!(memos.bound(&fleet_memo()).is_none());
}

#[test]
fn briefs_remembered_hold_until_that_sessions_launch_or_mail_changes() -> anyhow::Result<()> {
    let (memos, feed) = attached();
    let session = Uuid::new_v4();
    let other = Uuid::new_v4();
    let briefs = memos
        .briefs()
        .ok_or_else(|| anyhow::anyhow!("feed attached"))?;
    assert_eq!(briefs.get(session), None);
    briefs.remember(session, briefs.ticket(), ());
    assert_eq!(briefs.get(session), Some(()));

    feed.record(RowChangedTable::LaunchLaunches, Some(&other.to_string()));
    feed.record(RowChangedTable::Requests, Some(&session.to_string()));
    assert_eq!(briefs.get(session), Some(()), "unrelated changes keep it");

    feed.record(RowChangedTable::MailboxMessages, Some(&session.to_string()));
    assert_eq!(briefs.get(session), None, "its own mail changed");
    briefs.remember(session, briefs.ticket(), ());
    memos.record(RowChangedTable::LaunchLaunches, Some(session));
    assert_eq!(briefs.get(session), None, "a launch of its own was written");
    Ok(())
}

#[test]
fn fleets_remembered_are_dropped_by_any_fleet_change() -> anyhow::Result<()> {
    let (memos, feed) = attached();
    let session = Uuid::new_v4();
    let fleet_memo = fleet_memo();
    let fleets = memos
        .bound(&fleet_memo)
        .ok_or_else(|| anyhow::anyhow!("feed attached"))?;
    fleets.remember(session, fleets.ticket(), None);
    assert_eq!(fleets.get(session), Some(None));
    memos.record(RowChangedTable::LaunchFleets, None);
    assert_eq!(fleets.get(session), None);
    fleets.remember(session, fleets.ticket(), None);
    feed.resync();
    assert_eq!(fleets.get(session), None, "a resync drops everything");
    Ok(())
}

#[test]
fn a_change_between_ticket_and_remember_is_not_remembered() -> anyhow::Result<()> {
    let (memos, feed) = attached();
    let session = Uuid::new_v4();
    let briefs = memos
        .briefs()
        .ok_or_else(|| anyhow::anyhow!("feed attached"))?;
    let ticket = briefs.ticket();
    feed.record(RowChangedTable::LaunchLaunches, Some(&session.to_string()));
    briefs.remember(session, ticket, ());
    assert_eq!(briefs.get(session), None);

    let ticket = briefs.ticket();
    memos.forget_briefs();
    briefs.remember(session, ticket, ());
    assert_eq!(briefs.get(session), None, "a forget during the read wins");
    Ok(())
}

#[test]
fn forgetting_briefs_drops_every_remembered_session() -> anyhow::Result<()> {
    let (memos, _feed) = attached();
    let session = Uuid::new_v4();
    let briefs = memos
        .briefs()
        .ok_or_else(|| anyhow::anyhow!("feed attached"))?;
    briefs.remember(session, briefs.ticket(), ());
    memos.forget_briefs();
    assert_eq!(briefs.get(session), None);
    Ok(())
}
