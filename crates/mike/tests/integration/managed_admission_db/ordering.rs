use std::time::Duration;

use mike::launch::fleet::test_seams::durable;

use super::support::{Harness, command, phase};

#[tokio::test]
async fn production_spawn_share_guard_delays_durable_stop_until_it_is_released()
-> anyhow::Result<()> {
    let Some(h) = Harness::new().await? else {
        return Ok(());
    };
    let held = durable::retain_spawn_fence(h.persist.clone(), h.fleet.fleet_uuid, &h.run)
        .await
        .map_err(anyhow::Error::msg)?;
    let mut stop = Box::pin(h.stop_other());
    assert!(
        tokio::time::timeout(Duration::from_millis(100), &mut stop)
            .await
            .is_err()
    );
    let during = h
        .persist
        .find_fleet(&h.fleet.fleet_uuid.to_string())
        .await?
        .ok_or_else(|| anyhow::anyhow!("missing fixture fleet"))?;
    assert_eq!(during.state, "running");
    held.commit().await.map_err(anyhow::Error::msg)?;
    let stopped = tokio::time::timeout(Duration::from_secs(3), &mut stop).await??;
    assert_eq!(stopped.state, "stopping");
    drop(stop);
    assert!(
        durable::retain_spawn_fence(h.persist.clone(), h.fleet.fleet_uuid, &h.run)
            .await
            .is_err()
    );
    h.finish().await
}

#[tokio::test]
async fn stopping_wins_local_admission_race_without_creating_a_receipt() -> anyhow::Result<()> {
    let Some(h) = Harness::new().await? else {
        return Ok(());
    };
    let held = durable::local_stop_fence(&h.run).await;
    let mut begin = Box::pin(
        h.domain
            .begin(h.persist.clone(), h.request("touch must-not-run")),
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(100), &mut begin)
            .await
            .is_err()
    );
    let stopped = h.stop_other().await?;
    held.stopping_persisted();
    drop(held);
    assert!(
        tokio::time::timeout(Duration::from_secs(3), &mut begin)
            .await?
            .is_err()
    );
    drop(begin);
    assert_eq!(stopped.state, "stopping");
    assert!(h.persist.pending_fleet_commands(&h.run).await?.is_empty());
    assert!(durable::snapshot(&h.run).pending.is_empty());
    assert!(!h.repo.path().join("must-not-run").exists());
    h.finish().await
}

#[tokio::test]
async fn queued_receipt_insert_blocks_stop_and_cannot_be_a_completion_phantom() -> anyhow::Result<()>
{
    let Some(h) = Harness::new().await? else {
        return Ok(());
    };
    let mut table = h.other.pool().begin().await?;
    sqlx::query("LOCK TABLE launch.tree_checks IN SHARE MODE")
        .execute(&mut *table)
        .await?;
    let mut begin = Box::pin(
        h.domain
            .begin(h.persist.clone(), h.request("touch must-not-run")),
    );
    tokio::select! {
        _ = &mut begin => anyhow::bail!("queued insert did not remain behind the fixture table lock"),
        registered = phase(&h.run, "Registering") => registered?,
    }
    assert!(
        durable::snapshot(&h.run)
            .pending
            .iter()
            .all(|entry| entry.receipt.is_none())
    );
    let mut stop = Box::pin(h.stop_other());
    assert!(
        tokio::time::timeout(Duration::from_millis(100), &mut stop)
            .await
            .is_err()
    );
    table.commit().await?;
    let queued = tokio::time::timeout(Duration::from_secs(3), &mut begin)
        .await?
        .map_err(anyhow::Error::msg)?;
    let stopped = tokio::time::timeout(Duration::from_secs(3), &mut stop).await??;
    drop(begin);
    drop(stop);
    let pending = h.persist.pending_fleet_commands(&h.run).await?;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, queued.id());
    assert!(h.other.complete_fleet_run(&stopped, None).await?.is_none());
    let receipt = queued
        .cancel_without_spawn(h.repo.path().into())
        .await
        .map_err(anyhow::Error::msg)?;
    assert_eq!(receipt.exit_code, None);
    assert_eq!(receipt.state_after.as_deref(), Some("cancelled:unverified"));
    assert!(receipt.finished_at.is_some());
    assert!(!h.repo.path().join("must-not-run").exists());
    assert!(h.other.complete_fleet_run(&stopped, None).await?.is_some());
    h.finish().await
}

#[tokio::test]
async fn stop_after_receipt_before_spawn_records_cancelled_and_never_launches() -> anyhow::Result<()>
{
    let Some(h) = Harness::new().await? else {
        return Ok(());
    };
    let queued = h
        .domain
        .begin(h.persist.clone(), h.request("touch must-not-run"))
        .await
        .map_err(anyhow::Error::msg)?;
    h.stop_other().await?;
    assert!(!durable::snapshot(&h.run).stop_requested);
    let receipt = tokio::time::timeout(Duration::from_secs(5), queued.spawn(command(&h)))
        .await??
        .map_err(anyhow::Error::msg)?;
    assert_eq!(receipt.exit_code, None);
    assert_eq!(receipt.state_after.as_deref(), Some("cancelled:unverified"));
    assert!(
        receipt.output.contains("state or exact run"),
        "{}",
        receipt.output
    );
    assert!(!h.repo.path().join("must-not-run").exists());
    assert!(h.persist.pending_fleet_commands(&h.run).await?.is_empty());
    h.finish().await
}

#[tokio::test]
async fn stop_after_receipt_before_auxiliary_git_records_cancelled_without_launch()
-> anyhow::Result<()> {
    let Some(h) = Harness::new().await? else {
        return Ok(());
    };
    mike::launch::fleet::test_seams::control::git(h.repo.path(), &["init", "-q"])
        .await
        .map_err(anyhow::Error::msg)?;
    let queued = h
        .domain
        .begin(h.persist.clone(), h.request("git fixture marker"))
        .await
        .map_err(anyhow::Error::msg)?;
    h.stop_other().await?;
    let task = queued.spawn_git(
        command(&h),
        vec![
            "-c".into(),
            "alias.marker=!touch must-not-run".into(),
            "marker".into(),
        ],
    );
    let receipt = tokio::time::timeout(Duration::from_secs(5), task)
        .await??
        .map_err(anyhow::Error::msg)?;
    assert_eq!(receipt.exit_code, None);
    assert_eq!(receipt.state_after.as_deref(), Some("cancelled:unverified"));
    assert!(
        receipt
            .output
            .contains("state or exact run changed before launch"),
        "{}",
        receipt.output
    );
    assert!(!h.repo.path().join("must-not-run").exists());
    assert!(h.persist.pending_fleet_commands(&h.run).await?.is_empty());
    h.finish().await
}
