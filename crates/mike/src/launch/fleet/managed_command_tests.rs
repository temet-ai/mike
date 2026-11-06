use super::*;
use crate::launch::fleet::owned_execution::Registry;

#[tokio::test]
async fn shared_checkout_wait_cancels_before_either_check_or_gate_can_launch() -> anyhow::Result<()>
{
    let root = tempfile::tempdir()?;
    let registry = Registry::default();
    let fence = registry.fence("run").await;
    let check = fence.register("run#command").map_err(anyhow::Error::msg)?;
    let gate = fence.register("run#gate").map_err(anyhow::Error::msg)?;
    let held = crate::launch::fleet::gate_slot::queue::hold(root.path()).await;
    let check_owner = check.owner();
    let gate_owner = gate.owner();
    let first = hold_checkout_owned(&check_owner, root.path(), Duration::from_secs(2));
    let second = hold_checkout_owned(&gate_owner, root.path(), Duration::from_secs(2));
    tokio::pin!(first, second);
    assert!(
        tokio::time::timeout(Duration::from_millis(10), &mut first)
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(10), &mut second)
            .await
            .is_err()
    );
    fence.stopping_persisted();
    drop(fence);
    assert!(first.await.is_err());
    assert!(second.await.is_err());
    drop(held);
    check.receipt_recorded();
    gate.receipt_recorded();
    Ok(())
}

#[tokio::test]
async fn cancellation_has_a_terminal_nonpassing_state_and_preserves_prior_output()
-> anyhow::Result<()> {
    let registry = Registry::default();
    let registration = registry
        .fence("run")
        .await
        .register("run")
        .map_err(anyhow::Error::msg)?;
    let owner = registration.owner();
    owner.cancel("fleet stop requested");
    let mut code = Some(0);
    let mut output = "FAIL earlier detail\n".to_string();
    assert!(normalize_cancellation(&owner, &mut code, &mut output));
    assert_eq!(code, None);
    assert!(output.starts_with("FAIL earlier detail\n"));
    assert!(output.contains("fleet stop requested"));
    let before = output.clone();
    normalize_cancellation(&owner, &mut code, &mut output);
    assert_eq!(before, output);
    registration.receipt_recorded();
    Ok(())
}

#[tokio::test]
async fn timeout_waiting_for_checkout_is_an_error_and_does_not_unlock_owner() -> anyhow::Result<()>
{
    let root = tempfile::tempdir()?;
    let registry = Registry::default();
    let registration = registry
        .fence("run")
        .await
        .register("run")
        .map_err(anyhow::Error::msg)?;
    let held = crate::launch::fleet::gate_slot::queue::hold(root.path()).await;
    let answer =
        hold_checkout_owned(&registration.owner(), root.path(), Duration::from_millis(5)).await;
    assert!(answer.is_err());
    assert!(
        tokio::time::timeout(
            Duration::from_millis(5),
            crate::launch::fleet::gate_slot::queue::hold(root.path())
        )
        .await
        .is_err()
    );
    drop(held);
    registration.receipt_was_not_created();
    Ok(())
}

#[test]
fn a_slow_or_failing_state_read_does_not_cancel_until_it_stays_unverified() {
    let start = std::time::Instant::now();
    let mut unverified = Unverified::default();
    let unknown = || Activity::Unknown("pool timed out".into());
    assert_eq!(unverified.observe(unknown(), start), None);
    assert_eq!(
        unverified.observe(unknown(), start + UNVERIFIED_LIMIT - Duration::from_secs(1)),
        None
    );
    assert_eq!(
        unverified.observe(Activity::Active, start + UNVERIFIED_LIMIT),
        None
    );
    assert_eq!(
        unverified.observe(unknown(), start + UNVERIFIED_LIMIT),
        None
    );
    let late = start + UNVERIFIED_LIMIT * 2;
    assert!(
        unverified.observe(unknown(), late).is_some_and(|why| why
            .contains("could not be verified")
            && why.contains("pool timed out"))
    );
    assert_eq!(
        Unverified::default().observe(Activity::Ended("stopped".into()), start),
        Some("stopped".into())
    );
}
