use std::fs;
use std::path::Path;
use std::process::Command;

use super::{normalize_remote, repo_identity};

fn write(path: &Path, contents: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    Ok(())
}

fn git(dir: &Path, args: &[&str]) -> anyhow::Result<()> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.email=t@example.com", "-c", "user.name=t"])
        .args(args)
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn init_repo(path: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(path)?;
    git(path, &["init", "-q"])
}

#[test]
fn main_checkout_resolves_to_itself_with_origin() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let main = fs::canonicalize(tmp.path())?.join("repo");
    init_repo(&main)?;
    git(
        &main,
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:Temet-AI/temet-proxy.git",
        ],
    )?;
    fs::create_dir_all(main.join("crates/mike"))?;

    let id = repo_identity(&main.join("crates/mike"))
        .ok_or_else(|| anyhow::anyhow!("a main checkout has an identity"))?;

    assert_eq!(id.checkout_root, main);
    assert_eq!(id.main_root, main);
    assert_eq!(
        id.origin_url.as_deref(),
        Some("github.com/temet-ai/temet-proxy")
    );
    Ok(())
}

#[test]
fn linked_worktree_resolves_to_its_main_checkout() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let root = fs::canonicalize(tmp.path())?;
    let main = root.join("repo");
    let wt = root.join("wt/P1");
    init_repo(&main)?;
    git(
        &main,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/temet-ai/temet-proxy",
        ],
    )?;
    git(&main, &["commit", "-q", "--allow-empty", "-m", "initial"])?;
    git(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "linked",
            &wt.to_string_lossy(),
        ],
    )?;

    let id = repo_identity(&wt).ok_or_else(|| anyhow::anyhow!("a worktree has an identity"))?;

    assert_eq!(id.checkout_root, wt);
    assert_eq!(id.main_root, main);
    assert_eq!(
        id.origin_url.as_deref(),
        Some("github.com/temet-ai/temet-proxy")
    );
    Ok(())
}

#[test]
fn gitfile_with_a_separate_git_directory_keeps_its_identity() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let root = fs::canonicalize(tmp.path())?;
    let main = root.join("repo");
    let metadata = root.join("metadata");
    fs::create_dir_all(&main)?;
    git(
        &main,
        &[
            "init",
            "-q",
            "--separate-git-dir",
            &metadata.to_string_lossy(),
        ],
    )?;
    write(&main.join(".git"), "gitdir: ../metadata\n")?;
    let id = repo_identity(&main)
        .ok_or_else(|| anyhow::anyhow!("a separate git directory has an identity"))?;
    assert_eq!(id.checkout_root, main);
    assert_eq!(id.main_root, metadata);
    Ok(())
}

#[test]
fn worktree_with_a_dangling_gitdir_has_no_identity() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let wt = tmp.path().join("wt");
    write(&wt.join(".git"), "gitdir: /nowhere/.git/worktrees/wt\n")?;
    assert!(repo_identity(&wt).is_none());
    Ok(())
}

#[test]
fn gitfile_target_without_repository_metadata_has_no_identity() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let wt = tmp.path().join("wt");
    fs::create_dir_all(tmp.path().join("metadata"))?;
    write(&wt.join(".git"), "gitdir: ../metadata\n")?;
    assert!(repo_identity(&wt).is_none());
    Ok(())
}

#[test]
fn empty_git_directory_has_no_identity() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    fs::create_dir_all(tmp.path().join(".git"))?;
    assert!(repo_identity(tmp.path()).is_none());
    Ok(())
}

#[test]
fn empty_nested_git_directory_does_not_hide_a_valid_ancestor() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    init_repo(tmp.path())?;
    let nested = tmp.path().join("nested");
    fs::create_dir_all(nested.join(".git"))?;
    let id = repo_identity(&nested)
        .ok_or_else(|| anyhow::anyhow!("the valid ancestor has an identity"))?;
    assert_eq!(id.checkout_root, tmp.path());
    Ok(())
}

#[test]
fn invalid_head_does_not_identify_a_repository() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    init_repo(tmp.path())?;
    write(&tmp.path().join(".git/HEAD"), "not a git head\n")?;
    assert!(repo_identity(tmp.path()).is_none());
    Ok(())
}

#[cfg(unix)]
#[test]
fn checkout_symlink_keeps_its_alias() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let main = tmp.path().join("repo");
    let alias = tmp.path().join("alias");
    init_repo(&main)?;
    std::os::unix::fs::symlink(&main, &alias)?;
    let id =
        repo_identity(&alias).ok_or_else(|| anyhow::anyhow!("a checkout alias has an identity"))?;
    assert_eq!(id.checkout_root, alias);
    assert_eq!(id.main_root, alias);
    Ok(())
}

#[test]
fn folder_outside_any_repo_has_no_identity() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    assert!(repo_identity(&tmp.path().join("nowhere")).is_none());
    Ok(())
}

#[test]
fn ssh_and_https_remotes_normalize_alike() {
    assert_eq!(
        normalize_remote("git@github.com:temet-ai/temet-proxy.git"),
        normalize_remote("https://github.com/temet-ai/temet-proxy/")
    );
}
