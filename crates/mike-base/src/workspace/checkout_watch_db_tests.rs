use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use uuid::Uuid;

use crate::store::persistence::location_move::db_tests::Fixture;
use crate::workspace::checkouts::record_origins;

fn git(dir: &Path, args: &[&str]) -> anyhow::Result<()> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.email=t@example.com", "-c", "user.name=t"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
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
    git(path, &["init", "-q"])?;
    git(
        path,
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:temet-ai/moved.git",
        ],
    )?;
    git(path, &["commit", "-q", "--allow-empty", "-m", "init"])
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

struct Disk {
    _tmp: tempfile::TempDir,
    base: PathBuf,
}

impl Disk {
    fn new() -> anyhow::Result<Self> {
        let tmp = tempfile::tempdir()?;
        let base = fs::canonicalize(tmp.path())?;
        Ok(Self { _tmp: tmp, base })
    }
}

async fn gone(f: &Fixture, location: Uuid) -> anyhow::Result<bool> {
    Ok(sqlx::query_scalar(
        "SELECT gone_at IS NOT NULL FROM projects.locations WHERE location_uuid = $1",
    )
    .bind(location)
    .fetch_one(f.handle.pool())
    .await?)
}

async fn parent(f: &Fixture, location: Uuid) -> anyhow::Result<Option<Uuid>> {
    Ok(sqlx::query_scalar(
        "SELECT parent_location_uuid FROM projects.locations WHERE location_uuid = $1",
    )
    .bind(location)
    .fetch_one(f.handle.pool())
    .await?)
}

#[tokio::test]
async fn watch_checkouts_missing_main_with_one_same_origin_candidate_moves_it() -> anyhow::Result<()>
{
    let f = Fixture::new().await?;
    let disk = Disk::new()?;
    let (old_root, new_root) = (disk.base.join("old"), disk.base.join("new"));
    init_repo(&old_root)?;
    let old = f.location(&text(&old_root)).await?;
    let session = f.session(old).await?;
    record_origins(&f.handle, f.machine).await?;

    fs::rename(&old_root, &new_root)?;
    let arrived = f.location(&text(&new_root)).await?;
    let arrived_session = f.session(arrived).await?;
    record_origins(&f.handle, f.machine).await?;

    assert_eq!(f.root_of(old).await?, Some(text(&new_root)));
    assert_eq!(f.root_of(arrived).await?, None);
    assert_eq!(f.session_location(session).await?, Some(old));
    assert_eq!(f.session_location(arrived_session).await?, Some(old));
    assert!(!gone(&f, old).await?);
    Ok(())
}

#[tokio::test]
async fn watch_checkouts_removed_worktree_then_new_worktree_elsewhere_is_not_moved()
-> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let disk = Disk::new()?;
    let main_root = disk.base.join("main");
    let (first, second) = (disk.base.join("wt-one"), disk.base.join("wt-two"));
    init_repo(&main_root)?;
    git(&main_root, &["worktree", "add", "-q", &text(&first)])?;
    let main = f.location(&text(&main_root)).await?;
    let removed = f.location(&text(&first)).await?;
    record_origins(&f.handle, f.machine).await?;
    assert_eq!(parent(&f, removed).await?, Some(main));

    git(&main_root, &["worktree", "remove", &text(&first)])?;
    git(&main_root, &["worktree", "add", "-q", &text(&second)])?;
    let added = f.location(&text(&second)).await?;
    record_origins(&f.handle, f.machine).await?;

    assert_eq!(f.root_of(removed).await?, Some(text(&first)));
    assert!(gone(&f, removed).await?);
    assert_eq!(f.root_of(added).await?, Some(text(&second)));
    assert_eq!(parent(&f, added).await?, Some(main));
    assert_eq!(f.root_of(main).await?, Some(text(&main_root)));
    assert!(!gone(&f, main).await?);
    Ok(())
}

#[tokio::test]
async fn watch_checkouts_missing_main_with_two_candidates_is_left_gone() -> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let disk = Disk::new()?;
    let old_root = disk.base.join("old");
    init_repo(&old_root)?;
    let old = f.location(&text(&old_root)).await?;
    record_origins(&f.handle, f.machine).await?;

    fs::rename(&old_root, disk.base.join("renamed"))?;
    init_repo(&disk.base.join("clone"))?;
    f.location(&text(&disk.base.join("renamed"))).await?;
    f.location(&text(&disk.base.join("clone"))).await?;
    record_origins(&f.handle, f.machine).await?;

    assert_eq!(f.root_of(old).await?, Some(text(&old_root)));
    assert!(gone(&f, old).await?);
    Ok(())
}
