use crate::store::persistence::fleet_repo::FleetPersist as _;
use uuid::Uuid;

use super::seed::seed_run;
use super::{fixture, new_fleet};
use crate::change_feed::{ChangeFeed, spawn_change_feed};
use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::seed_fleet_running;

fn fed(handle: &PersistHandle) -> ChangeFeed {
    let feed = ChangeFeed::new();
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    spawn_change_feed(
        &feed,
        Some(handle),
        rx,
        tokio_util::sync::CancellationToken::new(),
        &tokio_util::task::TaskTracker::new(),
    );
    feed
}

async fn running(handle: &PersistHandle) -> anyhow::Result<usize> {
    Ok(handle
        .fleet()
        .list_fleets_in_states(&["running"])
        .await?
        .len())
}

#[tokio::test]
async fn fleets_in_states_with_a_fed_feed_are_kept_until_a_fleet_changes() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let other = PersistHandle::connect(&f.db.database_url).await?;
    let feed = fed(&f.handle);
    assert_eq!(running(&f.handle).await?, 0);

    let fleet = other
        .fleet()
        .insert_fleet(&new_fleet("memo", "/tmp/fleet-memo"))
        .await?;
    let run = Uuid::new_v4().to_string();
    seed_run(&other, &run).await?;
    seed_fleet_running(&other, fleet.fleet_uuid, &run).await?;
    assert_eq!(running(&f.handle).await?, 0);

    feed.record(RowChangedTable::LaunchLaunches, None);
    assert_eq!(running(&f.handle).await?, 0);

    feed.record(RowChangedTable::LaunchFleets, None);
    assert_eq!(running(&f.handle).await?, 1);
    f.db.cleanup().await
}

#[tokio::test]
async fn fleets_in_states_keep_each_set_of_states_apart() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let _feed = fed(&f.handle);
    let fleet = f
        .handle
        .fleet()
        .insert_fleet(&new_fleet("apart", "/tmp/fleet-memo"))
        .await?;
    let run = Uuid::new_v4().to_string();
    seed_run(&f.handle, &run).await?;
    seed_fleet_running(&f.handle, fleet.fleet_uuid, &run).await?;
    let fleets = f.handle.fleet();
    assert_eq!(fleets.list_fleets_in_states(&["running"]).await?.len(), 1);
    assert_eq!(fleets.list_fleets_in_states(&["draft"]).await?.len(), 0);
    assert_eq!(
        fleets
            .list_fleets_in_states(&["running", "stopping"])
            .await?
            .len(),
        1
    );
    f.db.cleanup().await
}
