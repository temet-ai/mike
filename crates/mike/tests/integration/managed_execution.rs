use std::path::Path;
use std::time::Duration;

use mike::launch::fleet::test_seams::execution::{self, Command, Domain, Execution};

fn command(cwd: &Path, text: &str) -> Command {
    Command {
        cwd: cwd.into(),
        text: text.into(),
        timeout: Duration::from_secs(10),
        log: None,
    }
}

async fn wait_for_phase(execution: &Execution, phase: &str) -> anyhow::Result<()> {
    tokio::time::timeout(Duration::from_secs(5), async {
        while execution.phase().as_deref() != Some(phase) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    Ok(())
}

async fn wait_for_file(path: &Path, needle: &str) -> anyhow::Result<()> {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !std::fs::read_to_string(path).is_ok_and(|text| text.contains(needle)) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    Ok(())
}

#[tokio::test]
async fn missing_managed_context_refuses_spawn_without_side_effects() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let (code, output) = execution::without_context(command(root.path(), "touch unexpected")).await;
    assert_eq!(code, None);
    assert!(
        output.contains("managed execution context is missing"),
        "{output}"
    );
    assert!(!root.path().join("unexpected").exists());
    Ok(())
}

#[tokio::test]
async fn running_cancellation_preserves_output_and_cleans_owned_descendants() -> anyhow::Result<()>
{
    let root = tempfile::tempdir()?;
    let log = root.path().join("output.log");
    let domain = Domain::new()?;
    let execution = domain.execution("running-cancel").await?;
    let mut work = command(
        root.path(),
        "(sleep 1; touch orphan-survived) & printf 'FAIL original\\n'; sleep 30",
    );
    work.log = Some(log.clone());
    let task = execution.spawn(work);
    wait_for_file(&log, "FAIL original").await?;
    execution.cancel("fixture cancellation");
    let (code, output) = tokio::time::timeout(Duration::from_secs(5), task).await??;
    execution.finish().await?;
    assert_eq!(code, None);
    assert!(output.starts_with("FAIL original\n"), "{output}");
    assert!(output.contains("fixture cancellation"), "{output}");
    assert_eq!(std::fs::read_to_string(&log)?, output);
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert!(!root.path().join("orphan-survived").exists());
    Ok(())
}

#[tokio::test]
async fn caller_abort_waits_for_supervisor_cleanup_before_releasing_root() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let log = root.path().join("output.log");
    let domain = Domain::new()?;
    let execution = domain.execution("caller-abort").await?;
    let private_root = execution.root().to_path_buf();
    let mut work = command(root.path(), "printf 'FAIL before-abort\\n'; sleep 30");
    work.log = Some(log.clone());
    let task = execution.spawn(work);
    drop(domain);
    wait_for_file(&log, "FAIL before-abort").await?;
    task.abort();
    let aborted = task.await;
    assert!(aborted.is_err_and(|error| error.is_cancelled()));
    assert!(private_root.exists());
    execution.finish().await?;
    assert!(!private_root.exists());
    let output = std::fs::read_to_string(&log)?;
    assert!(output.starts_with("FAIL before-abort\n"), "{output}");
    assert!(
        output.contains("command caller ended before cleanup completed"),
        "{output}"
    );
    Ok(())
}

#[tokio::test]
async fn stopping_a_checkout_waiter_never_spawns_it_or_unlocks_the_holder() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let held = execution::hold_checkout(root.path()).await;
    let domain = Domain::new()?;
    let execution = domain.execution("checkout-waiter").await?;
    let task = execution.spawn_queued(command(root.path(), "touch should-not-run"));
    wait_for_phase(&execution, "WaitingCheckout").await?;
    execution.stop().await?;
    let (code, output) = tokio::time::timeout(Duration::from_secs(5), task).await??;
    execution.finish().await?;
    assert_eq!(code, None);
    assert!(output.contains("cancelled"), "{output}");
    assert!(!root.path().join("should-not-run").exists());
    assert!(
        tokio::time::timeout(
            Duration::from_millis(20),
            execution::hold_checkout(root.path())
        )
        .await
        .is_err()
    );
    drop(held);
    let _available = tokio::time::timeout(
        Duration::from_secs(1),
        execution::hold_checkout(root.path()),
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn cancelling_a_host_admission_waiter_preserves_the_running_owner() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let domain = Domain::new()?;
    let holder = domain.execution("host-holder").await?;
    let waiter = domain.execution("host-waiter").await?;
    let owner_task = holder.spawn_declaring(command(root.path(), "printf running > owner-started; while test ! -e finish-owner; do sleep 0.01; done; printf kept"), "database");
    wait_for_file(&root.path().join("owner-started"), "running").await?;
    let queued_task =
        waiter.spawn_declaring(command(root.path(), "touch should-not-run"), "database");
    wait_for_phase(&waiter, "WaitingAdmission").await?;
    waiter.stop().await?;
    let (code, output) = tokio::time::timeout(Duration::from_secs(5), queued_task).await??;
    waiter.finish().await?;
    assert_eq!(code, None);
    assert!(output.contains("cancel"), "{output}");
    assert!(!root.path().join("should-not-run").exists());
    assert!(!owner_task.is_finished());
    std::fs::write(root.path().join("finish-owner"), "finish")?;
    let result = tokio::time::timeout(Duration::from_secs(5), owner_task).await??;
    holder.finish().await?;
    assert_eq!(result, (Some(0), "kept".into()));
    Ok(())
}

#[tokio::test]
async fn fixture_lifecycle_lock_serializes_state_changes() -> anyhow::Result<()> {
    execution::fixture_lifecycle_fence().await
}

#[tokio::test]
async fn fixture_identity_check_refuses_wrong_run_and_state() -> anyhow::Result<()> {
    execution::fixture_rejects_wrong_identity().await
}

#[tokio::test]
async fn auxiliary_git_cancellation_retains_scoped_leases_until_cleanup() -> anyhow::Result<()> {
    use std::os::fd::AsRawFd;

    let root = tempfile::tempdir()?;
    mike::launch::fleet::test_seams::control::git(root.path(), &["init", "-q"])
        .await
        .map_err(anyhow::Error::msg)?;
    let lock_path = root.path().join("lease");
    let file = std::fs::File::create(&lock_path)?;
    assert_eq!(
        unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
    let other = std::fs::File::open(&lock_path)?;
    let domain = Domain::new()?;
    let execution = domain.execution("auxiliary-git").await?;
    let task = execution.spawn_git_queued(
        root.path().into(),
        vec![
            "-c".into(),
            "alias.slow=!printf 'ready' > ready; (sleep 1; touch survived) & wait".into(),
            "slow".into(),
        ],
        file,
    );
    wait_for_file(&root.path().join("ready"), "ready").await?;
    assert_ne!(
        unsafe { libc::flock(other.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
    assert!(
        tokio::time::timeout(
            Duration::from_millis(20),
            execution::hold_checkout(root.path())
        )
        .await
        .is_err()
    );
    execution.stop().await?;
    let why = tokio::time::timeout(Duration::from_secs(5), task)
        .await??
        .err()
        .ok_or_else(|| anyhow::anyhow!("cancelled Git unexpectedly succeeded"))?;
    assert!(why.contains("cancelled"), "{why}");
    execution.finish().await?;
    let _checkout = tokio::time::timeout(
        Duration::from_secs(1),
        execution::hold_checkout(root.path()),
    )
    .await?;
    assert_eq!(
        unsafe { libc::flock(other.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert!(!root.path().join("survived").exists());
    Ok(())
}
