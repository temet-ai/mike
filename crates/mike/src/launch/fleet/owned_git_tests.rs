use super::*;

fn request(root: &Path, args: &[&str], limit: Duration) -> Request {
    Request {
        repo: root.into(),
        args: args.iter().map(|arg| (*arg).into()).collect(),
        context: None,
        cancelled: CancellationToken::new(),
        limit,
        disable_hooks: true,
    }
}

#[tokio::test]
async fn standalone_git_keeps_exact_arguments_without_a_fleet_context() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    assert!(managed_command::current().is_err());
    run(root.path(), &["init", "-q"])
        .await
        .map_err(anyhow::Error::msg)?;
    let output = run(
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
    assert!(managed_command::current().is_err());
    Ok(())
}

#[tokio::test]
async fn timeout_preserves_both_streams_and_stops_inherited_descendants() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    run(root.path(), &["init", "-q"])
        .await
        .map_err(anyhow::Error::msg)?;
    let alias = "alias.slow=!printf 'before out'; printf 'before err' >&2; (sleep 1; touch survived) & wait";
    let answer = run_request(request(
        root.path(),
        &["-c", alias, "slow"],
        Duration::from_millis(150),
    ))
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
    run(root.path(), &["init", "-q"])
        .await
        .map_err(anyhow::Error::msg)?;
    let alias = "alias.slow=!touch ready; (sleep 1; touch survived) & wait";
    let request = request(root.path(), &["-c", alias, "slow"], Duration::from_secs(5));
    let token = request.cancelled.clone();
    let task = tokio::spawn(run_request(request));
    tokio::time::timeout(Duration::from_secs(2), async {
        while !root.path().join("ready").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    task.abort();
    let result = task.await;
    assert!(result.is_err());
    assert!(token.is_cancelled());
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert!(!root.path().join("survived").exists());
    Ok(())
}

#[tokio::test]
async fn cancelled_request_does_not_launch_git_alias() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    run(root.path(), &["init", "-q"])
        .await
        .map_err(anyhow::Error::msg)?;
    let request = request(
        root.path(),
        &["-c", "alias.marker=!touch started", "marker"],
        Duration::from_secs(5),
    );
    request.cancelled.cancel();
    assert!(run_request(request).await.is_err());
    assert!(!root.path().join("started").exists());
    Ok(())
}

#[tokio::test]
async fn failed_git_preserves_stdout_and_stderr_without_a_fake_success() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    run(root.path(), &["init", "-q"])
        .await
        .map_err(anyhow::Error::msg)?;
    let answer = run(
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
