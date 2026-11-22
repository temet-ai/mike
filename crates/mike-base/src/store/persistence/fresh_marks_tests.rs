use chrono::{Duration, Utc};
use sqlx::postgres::PgPoolOptions;

use crate::change_feed::ChangeFeed;
use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::pool_classes::PoolClasses;

const KEY: &str = "test.fresh";
const WATCHED: &[RowChangedTable] = &[RowChangedTable::RejudgeRuns];

fn handle() -> anyhow::Result<PersistHandle> {
    let pool = PgPoolOptions::new().connect_lazy("postgres://localhost/fresh_marks_fixture")?;
    Ok(PersistHandle::from_pools(PoolClasses::single(pool)))
}

fn fed(handle: &PersistHandle) -> ChangeFeed {
    let feed = ChangeFeed::new();
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    crate::change_feed::spawn_change_feed(
        &feed,
        Some(handle),
        rx,
        tokio_util::sync::CancellationToken::new(),
        &tokio_util::task::TaskTracker::new(),
    );
    feed
}

#[tokio::test]
async fn is_fresh_without_a_feed_is_false() -> anyhow::Result<()> {
    let handle = handle()?;
    assert_eq!(handle.fresh_ticket(), None);
    handle.mark_fresh(KEY, handle.fresh_ticket(), None);
    assert!(!handle.is_fresh(KEY, WATCHED, Utc::now()));
    Ok(())
}

#[tokio::test]
async fn is_fresh_after_mark_holds_until_a_watched_table_changes() -> anyhow::Result<()> {
    let handle = handle()?;
    let feed = fed(&handle);
    assert!(!handle.is_fresh(KEY, WATCHED, Utc::now()), "never marked");
    handle.mark_fresh(KEY, handle.fresh_ticket(), None);
    assert!(handle.is_fresh(KEY, WATCHED, Utc::now()));
    feed.record(RowChangedTable::Requests, None);
    assert!(handle.is_fresh(KEY, WATCHED, Utc::now()), "unrelated table");
    feed.record(RowChangedTable::RejudgeRuns, None);
    assert!(!handle.is_fresh(KEY, WATCHED, Utc::now()));
    Ok(())
}

#[tokio::test]
async fn is_fresh_with_a_change_between_ticket_and_mark_is_false() -> anyhow::Result<()> {
    let handle = handle()?;
    let feed = fed(&handle);
    let ticket = handle.fresh_ticket();
    feed.record(RowChangedTable::RejudgeRuns, None);
    handle.mark_fresh(KEY, ticket, None);
    assert!(!handle.is_fresh(KEY, WATCHED, Utc::now()));
    Ok(())
}

#[tokio::test]
async fn is_fresh_past_its_deadline_is_false() -> anyhow::Result<()> {
    let handle = handle()?;
    let _feed = fed(&handle);
    let now = Utc::now();
    handle.mark_fresh(KEY, handle.fresh_ticket(), Some(now + Duration::seconds(30)));
    assert!(handle.is_fresh(KEY, WATCHED, now));
    assert!(!handle.is_fresh(KEY, WATCHED, now + Duration::seconds(31)));
    Ok(())
}

#[tokio::test]
async fn is_fresh_after_resync_or_forget_is_false() -> anyhow::Result<()> {
    let handle = handle()?;
    let feed = fed(&handle);
    handle.mark_fresh(KEY, handle.fresh_ticket(), None);
    feed.resync();
    assert!(!handle.is_fresh(KEY, WATCHED, Utc::now()), "resync");
    handle.mark_fresh(KEY, handle.fresh_ticket(), None);
    handle.forget_fresh(KEY);
    assert!(!handle.is_fresh(KEY, WATCHED, Utc::now()), "forgotten");
    Ok(())
}
