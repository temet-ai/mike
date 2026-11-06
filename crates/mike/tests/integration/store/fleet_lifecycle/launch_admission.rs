use std::time::Duration;

use mike::store::persistence::launch::{LaunchOutcome, LaunchSurfaceRefs, NewLaunch};
use uuid::Uuid;

use super::{fixture, running, stopping};

#[tokio::test]
async fn held_launch_admission_does_not_block_a_definition_save() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run = format!("held-save-{}", Uuid::new_v4());
    let row = running(&f.handle, &run).await?;
    let guard = f
        .handle
        .begin_fleet_launch(&run)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no launch guard"))?;
    let mut definition = row.definition.clone();
    definition["goal"] = serde_json::json!("saved while a seat opens");
    let saved = tokio::time::timeout(
        Duration::from_secs(2),
        f.handle
            .save_fleet_definition(row.fleet_uuid, &definition, &row.missing, row.updated_at),
    )
    .await??;
    assert!(
        saved.is_some(),
        "the definition save must not wait for the open"
    );
    drop(guard);
    Ok(())
}

#[tokio::test]
async fn held_launch_admission_orders_backend_open_before_stop() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run = format!("held-{}", Uuid::new_v4());
    let row = running(&f.handle, &run).await?;
    let mut launch = NewLaunch::new("herdr", "/tmp/held-launch");
    launch.run_id = Some(run.clone());
    let id = f.handle.insert_launch(&launch).await?;
    let guard = f
        .handle
        .begin_fleet_launch(&run)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no launch guard"))?;
    let mut stop = Box::pin(stopping(&f.handle, &row));
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut stop)
            .await
            .is_err()
    );
    guard
        .record_opened(
            id,
            &LaunchSurfaceRefs {
                workspace_ref: Some("held-workspace".into()),
                tab_ref: Some("held-tab".into()),
                ..LaunchSurfaceRefs::default()
            },
        )
        .await?;
    let opened_run = f
        .handle
        .find_run(&run)
        .await?
        .ok_or_else(|| anyhow::anyhow!("opened run disappeared"))?;
    assert_eq!(opened_run.workspace_ref.as_deref(), Some("held-workspace"));
    assert_eq!(opened_run.tab_ref.as_deref(), Some("held-tab"));
    let stopped = tokio::time::timeout(Duration::from_secs(2), stop).await??;
    let launches = f.handle.unsettled_fleet_launches(&run).await?;
    assert_eq!(launches.len(), 1);
    assert!(launches[0].opened);
    assert!(f.handle.begin_fleet_launch(&run).await.is_err());
    assert!(f.handle.complete_fleet_run(&stopped, None).await?.is_none());
    f.handle
        .record_fleet_launch_verified_exit(id, &run, "observed exit")
        .await?;
    assert!(f.handle.complete_fleet_run(&stopped, None).await?.is_some());
    f.db.cleanup().await
}

#[tokio::test]
async fn dropped_open_guard_leaves_pending_reservation_blocking_completion() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run = format!("pending-open-{}", Uuid::new_v4());
    let row = running(&f.handle, &run).await?;
    let mut launch = NewLaunch::new("herdr", "/tmp/pending-open");
    launch.run_id = Some(run.clone());
    let id = f.handle.insert_launch(&launch).await?;
    let guard = f
        .handle
        .begin_fleet_launch(&run)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no launch guard"))?;
    drop(guard);
    let stopped = stopping(&f.handle, &row).await?;
    assert!(f.handle.complete_fleet_run(&stopped, None).await?.is_none());
    let pending = f.handle.unsettled_fleet_launches(&run).await?;
    assert_eq!(pending.len(), 1);
    assert!(!pending[0].opened);
    assert_eq!(pending[0].result, "pending");
    f.handle
        .record_launch_outcome(
            id,
            &LaunchOutcome {
                result: "failed".into(),
                reason: Some("refused before backend open".into()),
                opened: false,
                delivered: None,
                confirmed: false,
                channel_check: None,
                stderr: None,
            },
        )
        .await?;
    assert!(f.handle.complete_fleet_run(&stopped, None).await?.is_some());
    f.db.cleanup().await
}

#[tokio::test]
async fn launch_guard_rejects_wrong_launch_run_and_unknown_run_has_no_guard() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let first = format!("guard-{}", Uuid::new_v4());
    let other = format!("other-{}", Uuid::new_v4());
    running(&f.handle, &first).await?;
    running(&f.handle, &other).await?;
    let mut launch = NewLaunch::new("herdr", "/tmp/wrong-run");
    launch.run_id = Some(other.clone());
    let id = f.handle.insert_launch(&launch).await?;
    let guard = f
        .handle
        .begin_fleet_launch(&first)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no launch guard"))?;
    assert!(
        guard
            .record_opened(id, &LaunchSurfaceRefs::default())
            .await
            .is_err()
    );
    assert!(!f.handle.unsettled_fleet_launches(&other).await?[0].opened);
    assert!(
        f.handle
            .begin_fleet_launch("no-fleet-owner")
            .await?
            .is_none()
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn guarded_run_metadata_preserves_closure_and_refuses_another_run() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run = format!("metadata-{}", Uuid::new_v4());
    running(&f.handle, &run).await?;
    let mut metadata = mike::store::persistence::launch::NewRun {
        run_id: run.clone(),
        project_path: None,
        backend: "herdr".into(),
        workspace_ref: Some("status-workspace".into()),
        tab_ref: None,
    };
    f.handle
        .begin_fleet_launch(&run)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no guard"))?
        .record_run(&metadata)
        .await?;
    assert_eq!(
        f.handle
            .find_run(&run)
            .await?
            .and_then(|row| row.workspace_ref),
        Some("status-workspace".into())
    );
    let wrong = f
        .handle
        .begin_fleet_launch(&run)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no guard"))?;
    metadata.run_id = "other".into();
    assert!(wrong.record_run(&metadata).await.is_err());
    metadata.run_id = run.clone();
    f.handle.close_run(&run).await?;
    let closed = f.handle.find_run(&run).await?;
    let guard = f
        .handle
        .begin_fleet_launch(&run)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no guard"))?;
    assert!(guard.record_run(&metadata).await.is_err());
    assert_eq!(f.handle.find_run(&run).await?, closed);
    f.db.cleanup().await
}
