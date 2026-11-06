use super::*;

#[tokio::test]
async fn stop_cancels_queued_and_executing_owners_without_touching_another_run()
-> anyhow::Result<()> {
    let registry = Registry::default();
    let first = registry.fence("first").await;
    let queued = first
        .register("first#command:x")
        .map_err(anyhow::Error::msg)?;
    let executing = first.register("first#gate:x").map_err(anyhow::Error::msg)?;
    queued.owner().receipt(11);
    queued.owner().phase(Phase::WaitingAdmission, None);
    executing.owner().receipt(12);
    executing.owner().phase(Phase::Executing, None);
    drop(first);
    let second = registry.fence("second").await;
    let unrelated = second.register("second").map_err(anyhow::Error::msg)?;
    drop(second);
    registry.fence("first").await.stopping_persisted();
    assert!(queued.owner().token().is_cancelled());
    assert!(executing.owner().token().is_cancelled());
    assert!(!unrelated.owner().token().is_cancelled());
    assert!(
        registry
            .fence("first")
            .await
            .register("first#late")
            .is_err()
    );
    assert_eq!(registry.snapshot("first").pending.len(), 2);
    queued.receipt_recorded();
    executing.receipt_recorded();
    assert!(
        registry
            .drain("first", Duration::from_secs(1))
            .await
            .locally_settled()
    );
    unrelated.receipt_was_not_created();
    Ok(())
}

#[tokio::test]
async fn cleanup_finishing_does_not_remove_a_receipt_still_being_recorded() -> anyhow::Result<()> {
    let registry = Registry::default();
    let fence = registry.fence("run").await;
    let registration = fence.register("run").map_err(anyhow::Error::msg)?;
    registration.owner().receipt(42);
    registration.owner().phase(Phase::RecordingReceipt, None);
    fence.stopping_persisted();
    drop(fence);
    let snapshot = registry.drain("run", Duration::from_millis(5)).await;
    assert!(!snapshot.locally_settled());
    assert_eq!(snapshot.pending[0].phase, Phase::RecordingReceipt);
    registration.receipt_recorded();
    assert!(
        registry
            .drain("run", Duration::from_secs(1))
            .await
            .locally_settled()
    );
    Ok(())
}

#[tokio::test]
async fn failed_or_dropped_receipt_owners_remain_visible_to_stop() -> anyhow::Result<()> {
    let registry = Registry::default();
    let fence = registry.fence("run").await;
    let blocked = fence.register("run#one").map_err(anyhow::Error::msg)?;
    blocked.owner().receipt(1);
    blocked.receipt_blocked("database unavailable".into());
    let interrupted = fence.register("run#two").map_err(anyhow::Error::msg)?;
    let process_owner = interrupted.owner();
    drop(interrupted);
    process_owner.phase(Phase::Cleaning, None);
    fence.stopping_persisted();
    drop(fence);
    let snapshot = registry.drain("run", Duration::from_secs(5)).await;
    assert_eq!(snapshot.pending.len(), 2);
    assert!(
        snapshot
            .pending
            .iter()
            .any(|entry| entry.phase == Phase::ReceiptBlocked)
    );
    assert!(
        snapshot
            .pending
            .iter()
            .any(|entry| entry.phase == Phase::Interrupted)
    );
    assert!(!snapshot.locally_settled());
    Ok(())
}

#[tokio::test]
async fn restart_with_no_live_registry_is_unknown() {
    let registry = Registry::default();
    let snapshot = registry
        .drain("persisted-stopping-run", Duration::from_secs(1))
        .await;
    assert!(!snapshot.known_in_this_process);
    assert!(!snapshot.locally_settled());
}

#[tokio::test]
async fn cancelled_individual_owner_does_not_cancel_shared_or_unrelated_work() -> anyhow::Result<()>
{
    let registry = Registry::default();
    let fence = registry.fence("run").await;
    let first = fence.register("run#shared").map_err(anyhow::Error::msg)?;
    let second = fence.register("run#other").map_err(anyhow::Error::msg)?;
    first.owner().cancel("caller interrupted");
    assert!(!second.owner().token().is_cancelled());
    assert!(!registry.snapshot("run").stop_requested);
    first.receipt_recorded();
    second.receipt_recorded();
    Ok(())
}

#[tokio::test]
async fn explicitly_initialized_empty_run_can_stop_but_pending_work_cannot_be_erased()
-> anyhow::Result<()> {
    let registry = Registry::default();
    let fence = registry.fence("new-run").await;
    fence.establish_known_empty().map_err(anyhow::Error::msg)?;
    fence.stopping_persisted();
    drop(fence);
    assert!(
        registry
            .drain("new-run", Duration::from_secs(1))
            .await
            .locally_settled()
    );
    let other = registry.fence("other").await;
    let pending = other.register("other").map_err(anyhow::Error::msg)?;
    assert!(other.establish_known_empty().is_err());
    pending.receipt_was_not_created();
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn interrupted_owner_keeps_admission_until_cleanup_explicitly_releases_it()
-> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    std::fs::set_permissions(
        root.path(),
        <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o700),
    )?;
    let path = root.path().canonicalize()?;
    let registry = Registry::default();
    let registration = registry
        .fence("run")
        .await
        .register("run")
        .map_err(anyhow::Error::msg)?;
    let owner = registration.owner();
    owner.receipt(15);
    let token = CancellationToken::new();
    let permit = crate::launch::fleet::build_admission::acquire(&path, &token).await?;
    owner.hold_permit(permit).map_err(anyhow::Error::msg)?;
    drop(registration);
    assert!(
        tokio::time::timeout(
            Duration::from_millis(10),
            crate::launch::fleet::build_admission::acquire(&path, &token)
        )
        .await
        .is_err()
    );
    assert_eq!(
        registry.snapshot("run").pending[0].phase,
        Phase::Interrupted
    );
    owner.release_cleaned_permit();
    let _next = tokio::time::timeout(
        Duration::from_secs(1),
        crate::launch::fleet::build_admission::acquire(&path, &token),
    )
    .await??;
    Ok(())
}
