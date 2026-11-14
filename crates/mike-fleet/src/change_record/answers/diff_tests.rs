use std::path::Path;

use super::diff::commit_diff;
use super::diff_git::{relative_path, run, text, valid_object};

async fn repository() -> anyhow::Result<tempfile::TempDir> {
    let dir = tempfile::tempdir()?;
    let repo = dir.path().to_string_lossy();
    for args in [
        vec!["init", "-b", "main"],
        vec!["config", "user.email", "test@test"],
        vec!["config", "user.name", "test"],
    ] {
        run(&repo, &args, false).await.map_err(anyhow::Error::msg)?;
    }
    std::fs::write(dir.path().join("a.txt"), "before\n")?;
    run(&repo, &["add", "a.txt"], false)
        .await
        .map_err(anyhow::Error::msg)?;
    run(&repo, &["commit", "-m", "first"], false)
        .await
        .map_err(anyhow::Error::msg)?;
    Ok(dir)
}

async fn head(dir: &Path) -> anyhow::Result<String> {
    let (bytes, _) = run(&dir.to_string_lossy(), &["rev-parse", "HEAD"], false)
        .await
        .map_err(anyhow::Error::msg)?;
    Ok(String::from_utf8(bytes)?.trim().to_string())
}

#[tokio::test]
async fn commit_diff_reads_the_requested_commit_without_touching_dirty_files() -> anyhow::Result<()>
{
    let dir = repository().await?;
    let first = head(dir.path()).await?;
    std::fs::write(dir.path().join("a.txt"), "uncommitted peer change\n")?;
    let answer = commit_diff(&dir.path().to_string_lossy(), &first[..8], None, 400)
        .await
        .map_err(anyhow::Error::msg)?;
    assert!(answer.unified_diff.contains("+before"));
    assert!(!answer.unified_diff.contains("peer change"));
    assert_eq!(answer.commit, Some(first));
    assert!(!answer.truncated);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt"))?,
        "uncommitted peer change\n"
    );
    Ok(())
}

#[tokio::test]
async fn commit_diff_marks_truncation_and_rejects_unresolved_or_option_inputs() -> anyhow::Result<()>
{
    let dir = repository().await?;
    let sha = head(dir.path()).await?;
    let repo = dir.path().to_string_lossy();
    let answer = commit_diff(&repo, &sha, Some("a.txt"), 2)
        .await
        .map_err(anyhow::Error::msg)?;
    assert!(answer.truncated);
    assert_eq!(answer.unified_diff.lines().count(), 2);
    for commit in ["--output=/tmp/leak", "HEAD", "deadbeef"] {
        assert!(commit_diff(&repo, commit, None, 400).await.is_err());
    }
    assert!(
        commit_diff(&repo, &sha, Some("../outside"), 400)
            .await
            .is_err()
    );
    Ok(())
}

#[test]
fn diff_boundaries_preserve_text_and_reject_lossy_decoding() -> anyhow::Result<()> {
    assert_eq!(
        text(b"one\ntwo\n", 2).map_err(anyhow::Error::msg)?,
        ("one\ntwo\n".into(), false)
    );
    assert_eq!(
        text(b"one\ntwo\n", 1).map_err(anyhow::Error::msg)?,
        ("one\n".into(), true)
    );
    assert!(text(&[0xff], 2).is_err());
    assert!(valid_object("AB1234"));
    for path in ["", "../secret", "/tmp/secret", "a/../secret"] {
        assert!(relative_path(path).is_err());
    }
    assert_eq!(
        relative_path("src/a.rs").map_err(anyhow::Error::msg)?,
        "src/a.rs"
    );
    Ok(())
}

#[tokio::test]
async fn recorded_empty_file_additions_and_deletions_are_not_hidden() -> anyhow::Result<()> {
    let dir = repository().await?;
    let repo = dir.path().to_string_lossy();
    std::fs::write(dir.path().join("empty.txt"), "")?;
    run(&repo, &["add", "empty.txt"], false)
        .await
        .map_err(anyhow::Error::msg)?;
    let (hash, _) = run(&repo, &["rev-parse", ":empty.txt"], false)
        .await
        .map_err(anyhow::Error::msg)?;
    let hash = String::from_utf8(hash)?;
    let (added, _) = super::diff::blob_diff(&repo, None, Some(hash.trim()))
        .await
        .map_err(anyhow::Error::msg)?;
    let (deleted, _) = super::diff::blob_diff(&repo, Some(hash.trim()), None)
        .await
        .map_err(anyhow::Error::msg)?;
    assert!(String::from_utf8(added)?.contains("new file mode"));
    assert!(String::from_utf8(deleted)?.contains("deleted file mode"));
    Ok(())
}
