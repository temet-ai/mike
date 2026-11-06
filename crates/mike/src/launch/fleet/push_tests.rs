use std::path::{Path, PathBuf};

use super::push::{check_pushable_with, dry_run, git_said, push, retry_with_helper};

async fn said(dir: &Path, args: &[&str]) -> anyhow::Result<String> {
    git_said(&dir.to_string_lossy(), args)
        .await
        .map_err(|why| anyhow::anyhow!("git {args:?}: {why}"))
}

struct Scratch {
    _root: tempfile::TempDir,
    fleet: PathBuf,
}

async fn scratch(origin_reachable: bool) -> anyhow::Result<Scratch> {
    let root = tempfile::tempdir()?;
    let origin = root.path().join("origin.git");
    std::fs::create_dir_all(&origin)?;
    said(&origin, &["init", "-q", "--bare", "-b", "main"]).await?;
    let url = if origin_reachable {
        origin.to_string_lossy().into_owned()
    } else {
        root.path()
            .join("nowhere.git")
            .to_string_lossy()
            .into_owned()
    };
    let fleet = root.path().join("fleet");
    std::fs::create_dir_all(&fleet)?;
    said(&fleet, &["init", "-q", "-b", "main"]).await?;
    said(&fleet, &["config", "user.email", "t@example.com"]).await?;
    said(&fleet, &["config", "user.name", "t"]).await?;
    said(&fleet, &["remote", "add", "origin", &url]).await?;
    std::fs::write(fleet.join("a.txt"), "one\n")?;
    said(&fleet, &["add", "a.txt"]).await?;
    said(&fleet, &["commit", "-q", "-m", "a"]).await?;
    Ok(Scratch { _root: root, fleet })
}

#[tokio::test]
async fn a_failed_push_returns_gits_own_words() -> anyhow::Result<()> {
    let s = scratch(false).await?;
    let repo = s.fleet.to_string_lossy().into_owned();
    let refused = push(&repo, "main").await;
    let words = refused.err().unwrap_or_default();
    assert!(
        words.contains("does not appear to be a git repository")
            || words.contains("Could not read from remote repository"),
        "{words}"
    );
    Ok(())
}

#[tokio::test]
async fn a_dry_run_push_from_a_checkout_with_a_reachable_remote_passes() -> anyhow::Result<()> {
    let s = scratch(true).await?;
    let repo = s.fleet.to_string_lossy().into_owned();
    assert_eq!(dry_run(&repo, "main").await, Ok(()));
    assert_eq!(check_pushable_with(&repo, "main", false).await, Ok(()));
    let on_the_remote = said(&s.fleet, &["ls-remote", "origin", "main"]).await?;
    assert!(
        on_the_remote.is_empty(),
        "a dry run pushes nothing: {on_the_remote}"
    );
    Ok(())
}

#[tokio::test]
async fn a_dry_run_push_without_a_reachable_remote_fails_in_gits_words() -> anyhow::Result<()> {
    let s = scratch(false).await?;
    let repo = s.fleet.to_string_lossy().into_owned();
    let refused = check_pushable_with(&repo, "main", true).await;
    let words = refused.err().unwrap_or_default();
    assert!(!words.is_empty());
    assert!(
        said(
            &s.fleet,
            &[
                "config",
                "--local",
                "--get",
                "credential.https://github.com.helper"
            ]
        )
        .await
        .is_err(),
        "a failure that is not about credentials must not set a helper"
    );
    Ok(())
}

#[tokio::test]
async fn a_credential_failure_with_gh_logged_in_sets_the_local_helper_and_tries_again()
-> anyhow::Result<()> {
    let s = scratch(true).await?;
    let repo = s.fleet.to_string_lossy().into_owned();
    let first =
        "fatal: could not read Username for 'https://github.com': terminal prompts disabled";
    let key = "credential.https://github.com.helper";
    let retried = retry_with_helper(&repo, "main", first.to_string(), true).await;
    assert_eq!(retried, Ok(()));
    let helper = said(&s.fleet, &["config", "--local", "--get", key]).await?;
    assert_eq!(helper, "!gh auth git-credential");
    Ok(())
}

#[tokio::test]
async fn a_credential_failure_without_gh_logged_in_stays_a_failure_in_gits_words()
-> anyhow::Result<()> {
    let s = scratch(true).await?;
    let repo = s.fleet.to_string_lossy().into_owned();
    let first = "fatal: could not read Username for 'https://github.com'";
    let retried = retry_with_helper(&repo, "main", first.to_string(), false).await;
    assert_eq!(retried, Err(first.to_string()));
    assert!(
        said(
            &s.fleet,
            &[
                "config",
                "--local",
                "--get",
                "credential.https://github.com.helper"
            ]
        )
        .await
        .is_err()
    );
    Ok(())
}
