use std::sync::atomic::{AtomicI64, Ordering};

use sqlx::postgres::PgPoolOptions;

use crate::change_feed::ChangeFeed;
use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::pool_classes::PoolClasses;

struct Lessons;
struct Threads;

fn handle() -> anyhow::Result<PersistHandle> {
    let pool = PgPoolOptions::new().connect_lazy("postgres://localhost/generation_memo_fixture")?;
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

async fn read<Tag: 'static>(handle: &PersistHandle, reads: &AtomicI64) -> i64 {
    let got: Result<i64, ()> = handle
        .kept_generation::<Tag, _, _, _>(|| async { Ok(reads.fetch_add(1, Ordering::SeqCst) + 1) })
        .await;
    got.unwrap_or(-1)
}

#[tokio::test]
async fn without_a_fed_feed_every_call_reads() -> anyhow::Result<()> {
    let handle = handle()?;
    let reads = AtomicI64::new(0);
    assert_eq!(read::<Lessons>(&handle, &reads).await, 1);
    assert_eq!(read::<Lessons>(&handle, &reads).await, 2);
    Ok(())
}

#[tokio::test]
async fn a_kept_generation_is_read_again_only_after_a_generation_changes() -> anyhow::Result<()> {
    let handle = handle()?;
    let feed = fed(&handle);
    let reads = AtomicI64::new(0);
    assert_eq!(read::<Lessons>(&handle, &reads).await, 1);
    assert_eq!(read::<Lessons>(&handle, &reads).await, 1);
    feed.record(RowChangedTable::Lessons, None);
    assert_eq!(
        read::<Lessons>(&handle, &reads).await,
        1,
        "other tables keep it"
    );
    feed.record(RowChangedTable::ChangeGenerations, None);
    assert_eq!(read::<Lessons>(&handle, &reads).await, 2);
    assert_eq!(read::<Lessons>(&handle, &reads).await, 2);
    Ok(())
}

#[tokio::test]
async fn each_tag_keeps_its_own_value() -> anyhow::Result<()> {
    let handle = handle()?;
    let _feed = fed(&handle);
    let lessons = AtomicI64::new(10);
    let threads = AtomicI64::new(20);
    assert_eq!(read::<Lessons>(&handle, &lessons).await, 11);
    assert_eq!(read::<Threads>(&handle, &threads).await, 21);
    assert_eq!(read::<Lessons>(&handle, &lessons).await, 11);
    Ok(())
}

#[tokio::test]
async fn a_kept_value_that_no_longer_holds_is_read_again() -> anyhow::Result<()> {
    let handle = handle()?;
    let _feed = fed(&handle);
    let reads = AtomicI64::new(0);
    let next = || async { Ok::<_, ()>(reads.fetch_add(1, Ordering::SeqCst) + 1) };
    let first = handle
        .kept_generation_while::<Lessons, _, _, _>(next, |v| *v > 1)
        .await;
    assert_eq!(first, Ok(1));
    let next = || async { Ok::<_, ()>(reads.fetch_add(1, Ordering::SeqCst) + 1) };
    let second = handle
        .kept_generation_while::<Lessons, _, _, _>(next, |v| *v > 1)
        .await;
    assert_eq!(second, Ok(2));
    let next = || async { Ok::<_, ()>(reads.fetch_add(1, Ordering::SeqCst) + 1) };
    let third = handle
        .kept_generation_while::<Lessons, _, _, _>(next, |v| *v > 1)
        .await;
    assert_eq!(third, Ok(2));
    Ok(())
}

struct Skills;

impl super::NamedGenerations for Skills {
    const NAMES: &'static [&'static str] = &["skill_installs", "agents"];
}

fn generation_bumped(name: &str) -> crate::store::models::ProxyEvent {
    crate::store::models::ProxyEvent::RowChanged {
        table: RowChangedTable::ChangeGenerations,
        id: name.to_owned(),
        session_id: None,
        op: crate::store::models::RowChangedOp::Update,
    }
}

async fn read_named(handle: &PersistHandle, reads: &AtomicI64) -> i64 {
    let got: Result<i64, ()> = handle
        .kept_while_named_generations_hold::<Skills, _, _, _>(|| async {
            Ok(reads.fetch_add(1, Ordering::SeqCst) + 1)
        })
        .await;
    got.unwrap_or(-1)
}

#[tokio::test]
async fn a_named_memo_is_read_again_only_when_one_of_its_generations_moves() -> anyhow::Result<()>
{
    let handle = handle()?;
    let feed = fed(&handle);
    let reads = AtomicI64::new(0);
    assert_eq!(read_named(&handle, &reads).await, 1);
    feed.observe(&generation_bumped("seat_memory_sources"));
    feed.record(RowChangedTable::Lessons, None);
    assert_eq!(read_named(&handle, &reads).await, 1, "other generations keep it");
    feed.observe(&generation_bumped("agents"));
    assert_eq!(read_named(&handle, &reads).await, 2);
    assert_eq!(read_named(&handle, &reads).await, 2);
    feed.observe(&generation_bumped("skill_installs"));
    assert_eq!(read_named(&handle, &reads).await, 3);
    Ok(())
}

#[tokio::test]
async fn a_named_memo_without_a_fed_feed_reads_every_time() -> anyhow::Result<()> {
    let handle = handle()?;
    let reads = AtomicI64::new(0);
    assert_eq!(read_named(&handle, &reads).await, 1);
    assert_eq!(read_named(&handle, &reads).await, 2);
    Ok(())
}
