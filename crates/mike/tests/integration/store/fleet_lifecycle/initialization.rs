use mike::store::persistence::fleet_repo::FleetPersist as _;
use mike::store::persistence::launch::NewRun;
use mike::store::persistence::launch_repo::LaunchPersist as _;
use uuid::Uuid;

use super::{fixture, starting, stopping};

fn new_run(id: &str) -> NewRun {
    NewRun {
        run_id: id.into(),
        project_path: Some("/tmp/atomic-fleet-run".into()),
        backend: "herdr".into(),
        workspace_ref: None,
        tab_ref: None,
    }
}

#[tokio::test]
async fn startup_binds_run_atomically_and_stop_prevents_late_upsert() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let start = starting(&f.handle, None).await?;
    let run = new_run(&format!("atomic-{}", Uuid::new_v4()));
    let initialized = f
        .handle
        .fleet()
        .initialize_fleet_run(&start, &run)
        .await?
        .ok_or_else(|| anyhow::anyhow!("initialization lost"))?;
    assert_eq!(initialized.run_id.as_deref(), Some(run.run_id.as_str()));
    assert!(f.handle.launch().find_run(&run.run_id).await?.is_some());
    let stopped = stopping(&f.handle, &initialized).await?;
    let done = f
        .handle
        .fleet()
        .complete_fleet_run(&stopped, None)
        .await?
        .ok_or_else(|| anyhow::anyhow!("completion lost"))?;
    let closed = f
        .handle
        .launch()
        .find_run(&run.run_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("run missing"))?;
    assert!(closed.closed_at.is_some());
    assert!(
        f.handle
            .fleet()
            .initialize_fleet_run(&initialized, &run)
            .await?
            .is_none()
    );
    assert_eq!(f.handle.launch().find_run(&run.run_id).await?, Some(closed));
    assert_eq!(
        f.handle
            .fleet()
            .find_fleet(&done.fleet_uuid.to_string())
            .await?,
        Some(done)
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn stop_before_initialization_does_not_create_orphan_run() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let start = starting(&f.handle, None).await?;
    stopping(&f.handle, &start).await?;
    let run = new_run(&format!("not-created-{}", Uuid::new_v4()));
    assert!(
        f.handle
            .fleet()
            .initialize_fleet_run(&start, &run)
            .await?
            .is_none()
    );
    assert!(f.handle.launch().find_run(&run.run_id).await?.is_none());
    f.db.cleanup().await
}

#[tokio::test]
async fn startup_cannot_rebind_or_steal_another_fleets_run() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let existing_run = format!("owned-{}", Uuid::new_v4());
    let owner = starting(&f.handle, Some(&existing_run)).await?;
    let second = starting(&f.handle, None).await?;
    assert!(
        f.handle
            .fleet()
            .initialize_fleet_run(&second, &new_run(&existing_run))
            .await
            .is_err()
    );
    let other = new_run(&format!("rebind-{}", Uuid::new_v4()));
    assert!(
        f.handle
            .fleet()
            .initialize_fleet_run(&owner, &other)
            .await
            .is_err()
    );
    assert!(f.handle.launch().find_run(&other.run_id).await?.is_none());
    let again = f
        .handle
        .fleet()
        .initialize_fleet_run(&owner, &new_run(&existing_run))
        .await?
        .ok_or_else(|| anyhow::anyhow!("idempotent initialization lost"))?;
    assert_eq!(again, owner);
    f.db.cleanup().await
}
