use crate::store::persistence::test_support::{seed_fleet_done, seed_fleet_running};
use serde_json::json;
use uuid::Uuid;

use super::seed::{seed_launch, seed_run, seed_session};
use super::{fixture, new_fleet};
use crate::store::persistence::test_support::seed_project;

#[tokio::test]
async fn session_lookup_finds_an_ended_fleet_outside_the_list_window() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run_id = format!("fleet-run-{}", Uuid::new_v4());
    seed_run(&f.handle, &run_id).await?;
    let fleet = f
        .handle
        .insert_fleet(&new_fleet("target", "/tmp/fleet-a"))
        .await?;
    seed_fleet_running(&f.handle, fleet.fleet_uuid, &run_id).await?;
    let session = Uuid::new_v4();
    let launch = seed_launch(&f.handle, &run_id, "acceptance", Some(session), true).await?;
    f.handle
        .record_launch_closed(launch, "complete", Some(true))
        .await?;
    let ended = seed_fleet_done(&f.handle, fleet.fleet_uuid).await?;
    f.handle
        .insert_fleet(&new_fleet("newer", "/tmp/fleet-a"))
        .await?;
    assert_ne!(
        f.handle.list_fleet_headers(Some("/tmp/fleet-a"), 1).await?[0].fleet_uuid,
        fleet.fleet_uuid
    );
    assert_eq!(f.handle.find_fleet_for_session(session).await?, Some(ended));
    assert_eq!(f.handle.find_fleet_for_session(Uuid::new_v4()).await?, None);
    f.db.cleanup().await
}

#[tokio::test]
async fn configured_acceptance_lookup_is_project_scoped_and_launch_membership_wins()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let project = seed_project(f.handle.pool(), "/tmp/fleet-a").await?;
    let session = Uuid::new_v4();
    let unrelated = Uuid::new_v4();
    seed_session(f.handle.pool(), project, session).await?;
    seed_session(f.handle.pool(), project, unrelated).await?;
    let mut configured = new_fleet("configured", "/tmp/fleet-a");
    configured.definition = json!({"acceptance": {"session": session.to_string()}});
    let expected = f.handle.insert_fleet(&configured).await?;
    f.handle
        .insert_fleet(&new_fleet("newer-unrelated", "/tmp/fleet-a"))
        .await?;
    configured.project_path = "/tmp/fleet-b".to_owned();
    f.handle.insert_fleet(&configured).await?;
    assert_eq!(
        f.handle.find_fleet_for_session(session).await?,
        Some(expected)
    );
    assert_eq!(f.handle.find_fleet_for_session(unrelated).await?, None);

    let run = format!("fleet-run-{}", Uuid::new_v4());
    seed_run(&f.handle, &run).await?;
    let launched = f
        .handle
        .insert_fleet(&new_fleet("launched", "/tmp/fleet-a"))
        .await?;
    let launched = seed_fleet_running(&f.handle, launched.fleet_uuid, &run).await?;
    seed_launch(&f.handle, &run, "worker", Some(session), true).await?;
    configured.project_path = "/tmp/fleet-a".to_owned();
    f.handle.insert_fleet(&configured).await?;
    assert_eq!(
        f.handle.find_fleet_for_session(session).await?,
        Some(launched)
    );
    f.db.cleanup().await
}
