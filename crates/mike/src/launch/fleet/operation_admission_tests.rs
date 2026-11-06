use super::super::operation::CountedResource;
use super::*;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

async fn acquire(root: &Path, names: &[&str], token: &CancellationToken) -> Result<Permit, String> {
    let claims: Vec<ResourceClaim> = names.iter().map(|name| (*name).into()).collect();
    super::acquire(root, &claims, token).await
}

fn counted(name: &str, capacity: u32) -> ResourceClaim {
    ResourceClaim::Counted(CountedResource {
        name: name.to_string(),
        capacity,
    })
}

fn private_root() -> anyhow::Result<tempfile::TempDir> {
    let root = tempfile::tempdir()?;
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700))?;
    Ok(root)
}

#[tokio::test]
async fn commands_that_declare_no_shared_resource_run_together() -> anyhow::Result<()> {
    let root = private_root()?;
    let token = CancellationToken::new();
    let first = acquire(root.path(), &[], &token)
        .await
        .map_err(anyhow::Error::msg)?;
    let second = tokio::time::timeout(Duration::from_secs(1), acquire(root.path(), &[], &token))
        .await?
        .map_err(anyhow::Error::msg)?;
    let declared = tokio::time::timeout(
        Duration::from_secs(1),
        acquire(root.path(), &["database"], &token),
    )
    .await?
    .map_err(anyhow::Error::msg)?;
    drop((first, second, declared));
    Ok(())
}

#[tokio::test]
async fn shared_resources_serialize_and_independent_resources_run_together() -> anyhow::Result<()> {
    let root = private_root()?;
    let token = CancellationToken::new();
    let held = acquire(root.path(), &["database", "web-cache"], &token)
        .await
        .map_err(anyhow::Error::msg)?;
    let independent = tokio::time::timeout(
        Duration::from_secs(1),
        acquire(root.path(), &["other-db"], &token),
    )
    .await?
    .map_err(anyhow::Error::msg)?;
    assert!(
        tokio::time::timeout(
            Duration::from_millis(20),
            acquire(root.path(), &["database"], &token)
        )
        .await
        .is_err()
    );
    drop(held);
    let _released = tokio::time::timeout(
        Duration::from_secs(1),
        acquire(root.path(), &["database"], &token),
    )
    .await?
    .map_err(anyhow::Error::msg)?;
    drop(independent);
    Ok(())
}

#[tokio::test]
async fn duplicate_resources_do_not_deadlock_and_cancellation_releases_partial_locks()
-> anyhow::Result<()> {
    let root = private_root()?;
    let token = CancellationToken::new();
    let held = acquire(root.path(), &["z", "z"], &token)
        .await
        .map_err(anyhow::Error::msg)?;
    let cancelled = CancellationToken::new();
    let resources = ["a", "z"];
    let waiting = acquire(root.path(), &resources, &cancelled);
    let cancel = async {
        tokio::time::sleep(Duration::from_millis(20)).await;
        cancelled.cancel();
    };
    let (result, ()) = tokio::join!(waiting, cancel);
    assert!(result.is_err());
    let _released =
        tokio::time::timeout(Duration::from_secs(1), acquire(root.path(), &["a"], &token))
            .await?
            .map_err(anyhow::Error::msg)?;
    drop(held);
    Ok(())
}

#[tokio::test]
async fn a_command_waiting_on_one_resource_holds_none_of_the_others() -> anyhow::Result<()> {
    let root = private_root()?;
    let token = CancellationToken::new();
    let ink = acquire(root.path(), &["ink-tests"], &token)
        .await
        .map_err(anyhow::Error::msg)?;
    let both = ["rust-build", "ink-tests"];
    let gate = tokio::spawn({
        let root = root.path().to_path_buf();
        let token = token.clone();
        async move { acquire(&root, &both, &token).await }
    });
    tokio::time::sleep(Duration::from_millis(150)).await;
    let cargo = tokio::time::timeout(
        Duration::from_secs(1),
        acquire(root.path(), &["rust-build"], &token),
    )
    .await?
    .map_err(anyhow::Error::msg)?;
    assert!(!gate.is_finished());
    drop((cargo, ink));
    let held = tokio::time::timeout(Duration::from_secs(2), gate)
        .await??
        .map_err(anyhow::Error::msg)?;
    drop(held);
    Ok(())
}

#[tokio::test]
async fn resource_directory_symlink_is_refused() -> anyhow::Result<()> {
    let root = private_root()?;
    let outside = tempfile::tempdir()?;
    let name = format!("resource-{}", hex::encode(Sha256::digest(b"db")));
    std::os::unix::fs::symlink(outside.path(), root.path().join(name))?;
    assert!(
        acquire(root.path(), &["db"], &CancellationToken::new())
            .await
            .is_err()
    );
    assert_eq!(std::fs::read_dir(outside.path())?.count(), 0);
    Ok(())
}

#[tokio::test]
async fn capacity_two_admits_two_of_three_requests_and_the_third_waits_for_a_release()
-> anyhow::Result<()> {
    let root = private_root()?;
    let token = CancellationToken::new();
    let claim = [counted("rust-build", 2)];
    let first = super::acquire(root.path(), &claim, &token)
        .await
        .map_err(anyhow::Error::msg)?;
    let second = tokio::time::timeout(
        Duration::from_secs(1),
        super::acquire(root.path(), &claim, &token),
    )
    .await?
    .map_err(anyhow::Error::msg)?;
    assert!(
        tokio::time::timeout(
            Duration::from_millis(100),
            super::acquire(root.path(), &claim, &token)
        )
        .await
        .is_err()
    );
    drop(first);
    let third = tokio::time::timeout(
        Duration::from_secs(1),
        super::acquire(root.path(), &claim, &token),
    )
    .await?
    .map_err(anyhow::Error::msg)?;
    drop((second, third));
    Ok(())
}

#[tokio::test]
async fn capacity_one_declared_as_a_count_is_exclusive() -> anyhow::Result<()> {
    let root = private_root()?;
    let token = CancellationToken::new();
    let claim = [counted("db", 1)];
    let held = super::acquire(root.path(), &claim, &token)
        .await
        .map_err(anyhow::Error::msg)?;
    assert!(
        tokio::time::timeout(
            Duration::from_millis(100),
            super::acquire(root.path(), &claim, &token)
        )
        .await
        .is_err()
    );
    drop(held);
    Ok(())
}

#[tokio::test]
async fn a_counted_request_waiting_on_a_full_resource_holds_no_slot_of_the_others()
-> anyhow::Result<()> {
    let root = private_root()?;
    let token = CancellationToken::new();
    let full = super::acquire(root.path(), &[counted("test-db", 1)], &token)
        .await
        .map_err(anyhow::Error::msg)?;
    let both = [counted("rust-build", 1), counted("test-db", 1)];
    let waiting = tokio::spawn({
        let root = root.path().to_path_buf();
        let token = token.clone();
        async move { super::acquire(&root, &both, &token).await }
    });
    tokio::time::sleep(Duration::from_millis(150)).await;
    let build = tokio::time::timeout(
        Duration::from_secs(1),
        super::acquire(root.path(), &[counted("rust-build", 1)], &token),
    )
    .await?
    .map_err(anyhow::Error::msg)?;
    assert!(!waiting.is_finished());
    drop((build, full));
    let held = tokio::time::timeout(Duration::from_secs(2), waiting)
        .await??
        .map_err(anyhow::Error::msg)?;
    drop(held);
    Ok(())
}

#[tokio::test]
async fn cancelling_a_waiting_counted_request_releases_what_it_took() -> anyhow::Result<()> {
    let root = private_root()?;
    let token = CancellationToken::new();
    let held = super::acquire(root.path(), &[counted("z", 1)], &token)
        .await
        .map_err(anyhow::Error::msg)?;
    let cancelled = CancellationToken::new();
    let claims = [counted("a", 2), counted("z", 1)];
    let waiting = super::acquire(root.path(), &claims, &cancelled);
    let cancel = async {
        tokio::time::sleep(Duration::from_millis(20)).await;
        cancelled.cancel();
    };
    let (result, ()) = tokio::join!(waiting, cancel);
    assert!(result.is_err());
    let first = super::acquire(root.path(), &[counted("a", 2)], &token)
        .await
        .map_err(anyhow::Error::msg)?;
    let second = tokio::time::timeout(
        Duration::from_secs(1),
        super::acquire(root.path(), &[counted("a", 2)], &token),
    )
    .await?
    .map_err(anyhow::Error::msg)?;
    drop((held, first, second));
    Ok(())
}
