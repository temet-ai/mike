use std::collections::HashSet;

use super::diff_git::run;
use super::remote::on_a_remote;

async fn git(repo: &str, args: &[&str]) -> anyhow::Result<String> {
    let (bytes, _) = run(repo, args, false).await.map_err(anyhow::Error::msg)?;
    Ok(String::from_utf8(bytes)?.trim().to_owned())
}

async fn commit(repo: &str, dir: &std::path::Path, name: &str) -> anyhow::Result<String> {
    std::fs::write(dir.join(name), name)?;
    git(repo, &["add", name]).await?;
    git(repo, &["commit", "-q", "-m", name]).await?;
    git(repo, &["rev-parse", "HEAD"]).await
}

#[tokio::test]
async fn only_commits_present_and_reachable_from_a_remote_count_as_on_a_remote()
-> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let remote = tempfile::tempdir()?;
    let repo = dir.path().to_string_lossy().into_owned();
    let bare = remote.path().to_string_lossy().into_owned();
    git(&repo, &["init", "-q", "-b", "main"]).await?;
    git(&repo, &["config", "user.email", "test@test"]).await?;
    git(&repo, &["config", "user.name", "test"]).await?;
    git(&bare, &["init", "-q", "--bare"]).await?;
    git(&repo, &["remote", "add", "origin", &bare]).await?;
    let pushed = commit(&repo, dir.path(), "pushed.txt").await?;
    git(&repo, &["push", "-q", "origin", "main"]).await?;
    let local = commit(&repo, dir.path(), "local.txt").await?;
    let missing = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_owned();

    let found = on_a_remote(&repo, &[pushed.clone(), local, missing]).await;

    assert_eq!(found, HashSet::from([pushed]));
    Ok(())
}

#[tokio::test]
async fn a_folder_that_is_not_a_repository_marks_nothing() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let found = on_a_remote(&dir.path().to_string_lossy(), &["abcdef12".to_owned()]).await;
    assert!(found.is_empty());
    Ok(())
}
