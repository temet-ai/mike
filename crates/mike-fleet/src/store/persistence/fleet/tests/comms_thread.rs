use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::test_support::seed_fleet_running;
use uuid::Uuid;

use super::seed::{seed_launch, seed_run, seed_session};
use super::{fixture, new_fleet};
use crate::store::persistence::mailbox::ThreadSummaryFilter;
use crate::store::persistence::test_support::seed_project;

#[tokio::test]
async fn fleet_comms_thread_collects_seat_mail_and_names_project_and_fleet() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let project_id = seed_project(f.handle.pool(), "/tmp/fleet-a").await?;
    let run_id = format!("fleet-run-{}", Uuid::new_v4());
    seed_run(&f.handle, &run_id).await?;
    let fleet = f
        .handle
        .fleet()
        .insert_fleet(&new_fleet("comms", "/tmp/fleet-a"))
        .await?;
    seed_fleet_running(&f.handle, fleet.fleet_uuid, &run_id).await?;
    let (worker, acceptance, outsider) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    for session in [worker, acceptance, outsider] {
        seed_session(f.handle.pool(), project_id, session).await?;
    }
    seed_launch(&f.handle, &run_id, "worker-T1", Some(worker), true).await?;
    seed_launch(&f.handle, &run_id, "acceptance", Some(acceptance), true).await?;

    let thread = f
        .handle
        .fleet()
        .ensure_fleet_comms_thread(fleet.fleet_uuid)
        .await?;
    assert_eq!(thread, Some(fleet.fleet_uuid));
    assert_eq!(
        f.handle
            .fleet()
            .ensure_fleet_comms_thread(fleet.fleet_uuid)
            .await?,
        Some(fleet.fleet_uuid)
    );

    f.handle
        .launch()
        .insert_message_with_context(Uuid::new_v4(), acceptance, Some(worker), "T1 done", &[])
        .await?;
    f.handle
        .launch()
        .insert_message_with_context(Uuid::new_v4(), worker, Some(outsider), "relay", &[])
        .await?;
    f.handle
        .launch()
        .insert_message_with_context(Uuid::new_v4(), outsider, Some(outsider), "elsewhere", &[])
        .await?;

    let filter = ThreadSummaryFilter {
        thread_uuid: Some(fleet.fleet_uuid),
        ..ThreadSummaryFilter::default()
    };
    let rows = f.handle.launch().fetch_thread_summaries(&filter).await?;
    let [row] = rows.as_slice() else {
        anyhow::bail!("expected one fleet thread, got {}", rows.len());
    };
    assert_eq!(row.message_count, 2);
    assert_eq!(row.fleet_slug.as_deref(), Some("comms"));
    assert_eq!(row.fleet_state.as_deref(), Some("running"));
    assert_eq!(row.project_path.as_deref(), Some("/tmp/fleet-a"));
    assert!(row.subject.starts_with("Fleet comms"));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn fleet_comms_thread_for_an_unknown_fleet_is_none() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    assert_eq!(
        f.handle
            .fleet()
            .ensure_fleet_comms_thread(Uuid::new_v4())
            .await?,
        None
    );
    f.db.cleanup().await?;
    Ok(())
}
