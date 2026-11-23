use crate::change_feed::ChangeFeed;
use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::location_move::db_tests::Fixture;

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

async fn roots(f: &Fixture) -> anyhow::Result<Vec<String>> {
    Ok(f.handle
        .checkout_rows(f.machine)
        .await?
        .into_iter()
        .map(|row| row.root_path)
        .collect())
}

#[tokio::test]
async fn checkout_rows_with_a_fed_feed_are_read_again_only_after_a_locations_notice()
-> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let feed = fed(&f.handle);
    f.location("/test/memo/a").await?;
    assert_eq!(roots(&f).await?, ["/test/memo/a"]);

    f.location("/test/memo/b").await?;
    assert_eq!(roots(&f).await?, ["/test/memo/a"], "no notice yet");
    feed.record(RowChangedTable::Requests, None);
    feed.record(RowChangedTable::CheckoutEdits, None);
    assert_eq!(roots(&f).await?, ["/test/memo/a"], "other tables keep it");

    feed.record(RowChangedTable::Locations, None);
    assert_eq!(roots(&f).await?, ["/test/memo/a", "/test/memo/b"]);
    Ok(())
}

#[tokio::test]
async fn checkout_rows_after_a_listener_resync_are_read_again() -> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let feed = fed(&f.handle);
    f.location("/test/memo/a").await?;
    assert_eq!(roots(&f).await?, ["/test/memo/a"]);
    f.location("/test/memo/b").await?;
    feed.resync();
    assert_eq!(roots(&f).await?, ["/test/memo/a", "/test/memo/b"]);
    Ok(())
}

#[tokio::test]
async fn checkout_rows_without_a_fed_feed_are_read_on_every_call() -> anyhow::Result<()> {
    let f = Fixture::new().await?;
    f.location("/test/memo/a").await?;
    assert_eq!(roots(&f).await?, ["/test/memo/a"]);
    f.location("/test/memo/b").await?;
    assert_eq!(roots(&f).await?, ["/test/memo/a", "/test/memo/b"]);
    Ok(())
}

#[tokio::test]
async fn location_lists_of_a_machine_come_from_its_kept_checkout_rows() -> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let feed = fed(&f.handle);
    let a = f.location("/test/memo/a").await?;
    f.handle
        .set_location_origin(a, "github.com/temet-ai/memo")
        .await?;
    feed.record(RowChangedTable::Locations, None);
    assert_eq!(roots(&f).await?, ["/test/memo/a"]);
    f.location("/test/memo/b").await?;

    let locations = f.handle.locations_on_machine(f.machine).await?;
    let [only] = locations.as_slice() else {
        anyhow::bail!("expected the kept single location, got {locations:?}");
    };
    assert_eq!(only.location_uuid, a);
    assert_eq!(only.origin_url.as_deref(), Some("github.com/temet-ai/memo"));
    let placed = f.handle.machine_location_roots(f.machine).await?;
    let pairs: Vec<(&str, i64)> = placed
        .iter()
        .map(|root| (root.root_path.as_str(), root.project_id))
        .collect();
    assert_eq!(pairs, [("/test/memo/a", f.project)]);
    Ok(())
}
