use serde_json::json;
use uuid::Uuid;

use super::seed::{seed_launch, seed_run, seed_session};
use super::{Fixture, fixture, new_fleet};
use crate::change_feed::ChangeFeed;
use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::{FleetHeaderRow, FleetRow};
use crate::store::persistence::test_support::{seed_fleet_running, seed_project};

async fn running_fleet(f: &Fixture, slug: &str) -> anyhow::Result<(String, FleetRow)> {
    let run = format!("fleet-run-{}", Uuid::new_v4());
    seed_run(&f.handle, &run).await?;
    let fleet = f
        .handle
        .insert_fleet(&new_fleet(slug, "/tmp/fleet-a"))
        .await?;
    let running = seed_fleet_running(&f.handle, fleet.fleet_uuid, &run).await?;
    Ok((run, running))
}

async fn known_session(f: &Fixture) -> anyhow::Result<Uuid> {
    let project = seed_project(f.handle.pool(), "/tmp/fleet-a").await?;
    let session = Uuid::new_v4();
    seed_session(f.handle.pool(), project, session).await?;
    f.handle.session_row_id(session).await?;
    Ok(session)
}

#[tokio::test]
async fn session_fleet_lookup_with_a_feed_is_served_from_memory_until_the_feed_moves()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let feed = ChangeFeed::new();
    f.handle.attach_change_feed(feed.clone());
    let other_machine = PersistHandle::connect(&f.db.database_url).await?;
    let session = known_session(&f).await?;
    let (run, running) = running_fleet(&f, "memo").await?;

    assert_eq!(f.handle.find_fleet_for_session(session).await?, None);
    seed_launch(&other_machine, &run, "worker", Some(session), false).await?;
    assert_eq!(
        f.handle.find_fleet_for_session(session).await?,
        None,
        "the other machine's launch has not reached the feed, so the answer comes from memory"
    );
    feed.record(RowChangedTable::LaunchLaunches, Some(&session.to_string()));
    assert_eq!(
        f.handle.find_fleet_for_session(session).await?,
        Some(running.clone())
    );
    assert_eq!(
        f.handle.find_fleet_header_for_session(session).await?,
        Some(FleetHeaderRow::from(&running))
    );

    let definition = json!({"seats": ["coordinator"], "goal": "memo"});
    let saved = f
        .handle
        .save_fleet_definition(
            running.fleet_uuid,
            &definition,
            &json!([]),
            running.updated_at,
        )
        .await?;
    assert!(saved.is_some());
    let found = f
        .handle
        .find_fleet_for_session(session)
        .await?
        .ok_or_else(|| anyhow::anyhow!("the seat's fleet disappeared"))?;
    assert_eq!(
        found.definition, definition,
        "this process's own write is read back"
    );

    other_machine
        .set_fleet_state(running.fleet_uuid, "stopping", None, None)
        .await?;
    feed.record(RowChangedTable::LaunchFleets, None);
    let found = f
        .handle
        .find_fleet_for_session(session)
        .await?
        .ok_or_else(|| anyhow::anyhow!("the seat's fleet disappeared"))?;
    assert_eq!(
        found.state, "stopping",
        "a fleet change on the feed drops the memo"
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn session_fleet_lookup_for_a_session_without_a_known_row_does_not_remember_absence()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    f.handle.attach_change_feed(ChangeFeed::new());
    let other_machine = PersistHandle::connect(&f.db.database_url).await?;
    let (run, running) = running_fleet(&f, "unknown").await?;
    let session = Uuid::new_v4();

    assert_eq!(f.handle.find_fleet_for_session(session).await?, None);
    seed_launch(&other_machine, &run, "worker", Some(session), false).await?;
    assert_eq!(
        f.handle.find_fleet_for_session(session).await?,
        Some(running)
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn session_fleet_header_lookup_without_a_feed_reads_launch_then_acceptance()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let (run, running) = running_fleet(&f, "header").await?;
    let seat = Uuid::new_v4();
    seed_launch(&f.handle, &run, "worker", Some(seat), false).await?;
    assert_eq!(
        f.handle.find_fleet_header_for_session(seat).await?,
        Some(FleetHeaderRow::from(&running))
    );

    let acceptance = known_session(&f).await?;
    let mut configured = new_fleet("configured", "/tmp/fleet-a");
    configured.definition = json!({"acceptance": {"session": acceptance.to_string()}});
    let expected = f.handle.insert_fleet(&configured).await?;
    assert_eq!(
        f.handle.find_fleet_header_for_session(acceptance).await?,
        Some(FleetHeaderRow::from(&expected))
    );
    assert_eq!(
        f.handle
            .find_fleet_header_for_session(Uuid::new_v4())
            .await?,
        None
    );
    f.db.cleanup().await
}
