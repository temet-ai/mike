use mike::store::persistence::fleet_repo::FleetPersist as _;
use std::time::Duration;

use mike::launch::fleet::test_seams::{control, durable};
use tokio_util::sync::CancellationToken;

use super::support::{Harness, command, file, phase};

#[tokio::test]
async fn another_persistence_owner_stops_an_admission_waiter_without_local_token_access()
-> anyhow::Result<()> {
    let Some(h) = Harness::new().await? else {
        return Ok(());
    };
    let token = CancellationToken::new();
    let held = control::acquire_resource(&h.domain.path()?, "database", &token)
        .await
        .map_err(anyhow::Error::msg)?;
    let queued = h
        .domain
        .begin(h.persist.clone(), h.request("touch must-not-run"))
        .await
        .map_err(anyhow::Error::msg)?
        .declaring("database");
    let task = queued.spawn(command(&h));
    phase(&h.run, "WaitingAdmission").await?;
    let stopped = h.stop_other().await?;
    assert!(!durable::snapshot(&h.run).stop_requested);
    assert!(
        h.other
            .fleet()
            .complete_fleet_run(&stopped, None)
            .await?
            .is_none()
    );
    let receipt = tokio::time::timeout(Duration::from_secs(8), task)
        .await??
        .map_err(anyhow::Error::msg)?;
    assert_eq!(receipt.exit_code, None);
    assert_eq!(receipt.state_after.as_deref(), Some("cancelled:unverified"));
    assert!(receipt.finished_at.is_some());
    assert!(
        receipt.output.contains("fleet state or exact run"),
        "{}",
        receipt.output
    );
    assert!(!h.repo.path().join("must-not-run").exists());
    assert!(!durable::snapshot(&h.run).stop_requested);
    assert!(durable::snapshot(&h.run).pending.is_empty());
    assert!(
        tokio::time::timeout(
            Duration::from_millis(50),
            control::acquire_resource(&h.domain.path()?, "database", &token)
        )
        .await
        .is_err()
    );
    drop(held);
    assert!(
        h.other
            .fleet()
            .complete_fleet_run(&stopped, None)
            .await?
            .is_some()
    );
    h.finish().await
}

#[tokio::test]
async fn another_persistence_owner_stops_running_work_and_preserves_terminal_output()
-> anyhow::Result<()> {
    let Some(h) = Harness::new().await? else {
        return Ok(());
    };
    let queued = h.domain.begin(h.persist.clone(), h.request("printf 'FAIL before stop\\n'; printf ready > ready; (sleep 6; touch survived) & wait"))
        .await.map_err(anyhow::Error::msg)?;
    let task = queued.spawn(command(&h));
    file(&h.repo.path().join("ready"), "ready").await?;
    let stopped = h.stop_other().await?;
    assert!(!durable::snapshot(&h.run).stop_requested);
    let receipt = tokio::time::timeout(Duration::from_secs(8), task)
        .await??
        .map_err(anyhow::Error::msg)?;
    assert_eq!(receipt.exit_code, None);
    assert_eq!(receipt.state_after.as_deref(), Some("cancelled:unverified"));
    assert!(
        receipt.output.starts_with("FAIL before stop\n"),
        "{}",
        receipt.output
    );
    assert!(
        receipt.output.contains("fleet state or exact run"),
        "{}",
        receipt.output
    );
    assert!(receipt.finished_at.is_some());
    assert!(
        h.persist
            .fleet()
            .pending_fleet_commands(&h.run)
            .await?
            .is_empty()
    );
    assert!(
        h.other
            .fleet()
            .complete_fleet_run(&stopped, None)
            .await?
            .is_some()
    );
    tokio::time::sleep(Duration::from_secs(6)).await;
    assert!(!h.repo.path().join("survived").exists());
    h.finish().await
}

#[tokio::test]
async fn spawn_verification_failure_cancels_with_its_distinct_cause() -> anyhow::Result<()> {
    let Some(h) = Harness::new().await? else {
        return Ok(());
    };
    let queued = h
        .domain
        .begin(h.persist.clone(), h.request("touch must-not-run"))
        .await
        .map_err(anyhow::Error::msg)?
        .with_receipt_writer(h.other.clone());
    h.persist.pool().close().await;
    let receipt = tokio::time::timeout(Duration::from_secs(30), queued.spawn(command(&h)))
        .await??
        .map_err(anyhow::Error::msg)?;
    assert_eq!(receipt.exit_code, None);
    assert_eq!(receipt.state_after.as_deref(), Some("cancelled:unverified"));
    assert!(
        receipt.output.contains("spawn verification unavailable"),
        "{}",
        receipt.output
    );
    assert!(!h.repo.path().join("must-not-run").exists());
    assert!(
        h.other
            .fleet()
            .pending_fleet_commands(&h.run)
            .await?
            .is_empty()
    );
    h.finish().await
}

#[tokio::test]
async fn auxiliary_spawn_verification_failure_cancels_with_its_distinct_cause() -> anyhow::Result<()>
{
    let Some(h) = Harness::new().await? else {
        return Ok(());
    };
    let queued = h
        .domain
        .begin(h.persist.clone(), h.request("git fixture marker"))
        .await
        .map_err(anyhow::Error::msg)?
        .with_receipt_writer(h.other.clone());
    h.persist.pool().close().await;
    let task = queued.spawn_git(
        command(&h),
        vec![
            "-c".into(),
            "alias.marker=!touch must-not-run".into(),
            "marker".into(),
        ],
    );
    let receipt = tokio::time::timeout(Duration::from_secs(30), task)
        .await??
        .map_err(anyhow::Error::msg)?;
    assert_eq!(receipt.exit_code, None);
    assert_eq!(receipt.state_after.as_deref(), Some("cancelled:unverified"));
    assert!(
        receipt.output.contains("spawn verification unavailable"),
        "{}",
        receipt.output
    );
    assert!(!h.repo.path().join("must-not-run").exists());
    assert!(
        h.other
            .fleet()
            .pending_fleet_commands(&h.run)
            .await?
            .is_empty()
    );
    h.finish().await
}
