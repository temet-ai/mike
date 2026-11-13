use super::*;
use crate::launch::fleet::owned_execution::Registry;

fn private_dir() -> anyhow::Result<tempfile::TempDir> {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir()?;
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))?;
    Ok(dir)
}

fn fixture(slot: &std::path::Path, text: &str) -> anyhow::Result<group::ChildGroup> {
    let child = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(text)
        .current_dir(slot)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .process_group(0)
        .spawn()?;
    group::ChildGroup::new(child).map_err(anyhow::Error::msg)
}

async fn exercise(
    owner: Owner,
    slot: PathBuf,
    text: &str,
) -> anyhow::Result<(Option<i32>, String)> {
    let _permit = crate::launch::fleet::build_admission::acquire(&slot, owner.token()).await?;
    let mut child = fixture(&slot, text)?;
    let mut output = child
        .child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("missing output"))?;
    let command = Command {
        slot,
        text: text.into(),
        run: "fixture".into(),
        budget: (Duration::from_secs(5), "fixture".into()),
        log: None,
        setting: None,
    };
    let mut capture = capture::Capture::new(None);
    let why = collect(&owner, &mut child, &mut output, &mut capture, &command).await;
    let (code, complete) = tokio::join!(
        child.terminate_and_reap(Some(&owner)),
        capture.drain(&mut output)
    );
    let code = code.map_err(anyhow::Error::msg)?;
    if let Some(why) = &why {
        capture.diagnostic(why);
    }
    Ok((
        if complete && why.is_none() {
            code
        } else {
            None
        },
        capture.text(),
    ))
}

#[tokio::test]
async fn cancellation_preserves_output_kills_only_owned_group_and_releases_after_reap()
-> anyhow::Result<()> {
    let dir = private_dir()?;
    let registry = Registry::default();
    let registration = registry
        .fence("run")
        .await
        .register("run")
        .map_err(anyhow::Error::msg)?;
    let owner = registration.owner();
    let cancellation = owner.clone();
    let slot = dir.path().canonicalize()?;
    let mut unrelated = fixture(&slot, "sleep 30")?;
    let task = tokio::spawn(async move {
        exercise(
            owner,
            slot,
            "printf 'partial output\\n'; touch ready; (sleep 1; touch escaped) & wait",
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(2), async {
        while !dir.path().join("ready").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    cancellation.cancel("fixture stop");
    let (code, output) = tokio::time::timeout(Duration::from_secs(3), task).await???;
    assert_eq!(code, None);
    assert!(output.starts_with("partial output\n"), "{output}");
    assert!(output.contains("fixture stop"), "{output}");
    assert!(!unrelated.exited_unreaped().map_err(anyhow::Error::msg)?);
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert!(!dir.path().join("escaped").exists());
    let _next = crate::launch::fleet::build_admission::acquire(
        &dir.path().canonicalize()?,
        &tokio_util::sync::CancellationToken::new(),
    )
    .await?;
    unrelated
        .terminate_and_reap(Some(&cancellation))
        .await
        .map_err(anyhow::Error::msg)?;
    registration.receipt_recorded();
    Ok(())
}

#[tokio::test]
async fn an_unverified_cleanup_settles_once_the_group_is_gone() -> anyhow::Result<()> {
    let dir = private_dir()?;
    let registry = Registry::default();
    let registration = registry
        .fence("run")
        .await
        .register("run")
        .map_err(anyhow::Error::msg)?;
    let owner = registration.owner();
    let slot = dir.path().canonicalize()?;
    let mut child = fixture(&slot, "exit 3")?;
    let code = tokio::time::timeout(
        Duration::from_secs(5),
        settle(&owner, &mut child, Err("census failed".into())),
    )
    .await?
    .map_err(anyhow::Error::msg)?;
    assert_eq!(code, Some(3));
    let state = registry.snapshot("run");
    assert!(
        state.pending[0]
            .detail
            .as_deref()
            .is_some_and(|detail| detail.starts_with("census failed")),
        "{:?}",
        state.pending
    );
    registration.receipt_recorded();
    assert!(registry.snapshot("run").pending.is_empty());
    Ok(())
}

#[tokio::test]
async fn normal_exit_is_observed_unreaped_and_original_status_is_kept() -> anyhow::Result<()> {
    let dir = private_dir()?;
    let registry = Registry::default();
    let registration = registry
        .fence("run")
        .await
        .register("run")
        .map_err(anyhow::Error::msg)?;
    let (code, output) = exercise(
        registration.owner(),
        dir.path().canonicalize()?,
        "printf 'out\\n'; exit 7",
    )
    .await?;
    assert_eq!((code, output.as_str()), (Some(7), "out\n"));
    registration.receipt_recorded();
    Ok(())
}

#[tokio::test]
async fn cancelled_admission_waiter_never_reaches_spawn() -> anyhow::Result<()> {
    let dir = private_dir()?;
    let registry = Registry::default();
    let registration = registry
        .fence("run")
        .await
        .register("run")
        .map_err(anyhow::Error::msg)?;
    let owner = registration.owner();
    let fresh = tokio_util::sync::CancellationToken::new();
    let held =
        crate::launch::fleet::build_admission::acquire(&dir.path().canonicalize()?, &fresh).await?;
    let slot = dir.path().canonicalize()?;
    let task_owner = owner.clone();
    let task =
        tokio::spawn(async move { exercise(task_owner, slot, "touch should-not-start").await });
    tokio::time::sleep(Duration::from_millis(50)).await;
    owner.cancel("stopped while queued");
    assert!(
        tokio::time::timeout(Duration::from_secs(2), task)
            .await??
            .is_err()
    );
    drop(held);
    assert!(!dir.path().join("should-not-start").exists());
    registration.receipt_recorded();
    Ok(())
}
