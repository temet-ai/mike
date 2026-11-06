use mike::launch::fleet::test_seams::control::{git, git_with_limit, has_managed_context};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn standalone_git_keeps_exact_arguments_without_a_fleet_context() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    assert!(!has_managed_context());
    git(root.path(), &["init", "-q"])
        .await
        .map_err(anyhow::Error::msg)?;
    let output = git(
        root.path(),
        &[
            "-c",
            "alias.echo=!f() { printf '%s' \"$1\"; }; f",
            "echo",
            "spaces and 'quotes'",
        ],
    )
    .await
    .map_err(anyhow::Error::msg)?;
    assert_eq!(output, "spaces and 'quotes'");
    assert!(!has_managed_context());
    Ok(())
}

#[tokio::test]
async fn timeout_preserves_both_streams_and_stops_inherited_descendants() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    git(root.path(), &["init", "-q"])
        .await
        .map_err(anyhow::Error::msg)?;
    let alias = "alias.slow=!printf 'before out'; printf 'before err' >&2; (sleep 1; touch survived) & wait";
    let answer = git_with_limit(
        root.path(),
        &["-c", alias, "slow"],
        Duration::from_millis(150),
        CancellationToken::new(),
    )
    .await;
    let why = answer
        .err()
        .ok_or_else(|| anyhow::anyhow!("fixture should time out"))?;
    assert!(why.contains("timed out"), "{why}");
    assert!(why.contains("before out"), "{why}");
    assert!(why.contains("before err"), "{why}");
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert!(!root.path().join("survived").exists());
    Ok(())
}

#[tokio::test]
async fn cancelled_standalone_caller_retains_detached_cleanup_until_reap() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    git(root.path(), &["init", "-q"])
        .await
        .map_err(anyhow::Error::msg)?;
    let token = CancellationToken::new();
    let child_token = token.clone();
    let path = root.path().to_path_buf();
    let task = tokio::spawn(async move {
        let alias = "alias.slow=!touch ready; (sleep 1; touch survived) & wait";
        git_with_limit(
            &path,
            &["-c", alias, "slow"],
            Duration::from_secs(5),
            child_token,
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(2), async {
        while !root.path().join("ready").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    task.abort();
    assert!(task.await.is_err());
    assert!(token.is_cancelled());
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert!(!root.path().join("survived").exists());
    Ok(())
}

#[tokio::test]
async fn cancelled_request_does_not_launch_git_alias() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    git(root.path(), &["init", "-q"])
        .await
        .map_err(anyhow::Error::msg)?;
    let token = CancellationToken::new();
    token.cancel();
    assert!(
        git_with_limit(
            root.path(),
            &["-c", "alias.marker=!touch started", "marker"],
            Duration::from_secs(5),
            token
        )
        .await
        .is_err()
    );
    assert!(!root.path().join("started").exists());
    Ok(())
}

#[tokio::test]
async fn failed_git_preserves_stdout_and_stderr_without_a_fake_success() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    git(root.path(), &["init", "-q"])
        .await
        .map_err(anyhow::Error::msg)?;
    let answer = git(
        root.path(),
        &[
            "-c",
            "alias.fail=!printf 'out'; printf 'err' >&2; exit 7",
            "fail",
        ],
    )
    .await;
    let why = answer
        .err()
        .ok_or_else(|| anyhow::anyhow!("fixture should fail"))?;
    assert!(why.contains("Some(7)"), "{why}");
    assert!(why.contains("out"), "{why}");
    assert!(why.contains("err"), "{why}");
    Ok(())
}
