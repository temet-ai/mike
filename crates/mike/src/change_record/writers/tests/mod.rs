mod events;
mod hooks;
mod paths;
mod words;

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;
use uuid::Uuid;

use crate::change_record::{ChangeEvent, ChangeQuery};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::change_record::test_seed;
use crate::store::persistence::test_support::{self, TestDbHandle};

pub(super) struct Fixture {
    pub db: TestDbHandle,
    pub handle: PersistHandle,
}

pub(super) async fn fixture() -> anyhow::Result<Option<Fixture>> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(None);
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    Ok(Some(Fixture { db, handle }))
}

pub(super) struct Repo {
    pub dir: TempDir,
    pub first_commit: String,
}

impl Repo {
    pub(super) fn path(&self) -> &Path {
        self.dir.path()
    }

    pub(super) fn checkout(&self) -> String {
        self.dir.path().to_string_lossy().into_owned()
    }
}

fn run_git(dir: &Path, args: &[&str]) -> anyhow::Result<String> {
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
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub(super) fn repo() -> anyhow::Result<Repo> {
    let dir = tempfile::tempdir()?;
    run_git(dir.path(), &["init", "-q"])?;
    std::fs::create_dir_all(dir.path().join("src"))?;
    std::fs::write(dir.path().join("src/lib.rs"), "pub fn a() {}\n")?;
    std::fs::write(dir.path().join("README.md"), "hello\n")?;
    run_git(dir.path(), &["add", "."])?;
    run_git(dir.path(), &["commit", "-q", "-m", "first"])?;
    let first_commit = run_git(dir.path(), &["rev-parse", "HEAD"])?;
    Ok(Repo { dir, first_commit })
}

pub(super) fn plain_dir() -> anyhow::Result<TempDir> {
    Ok(tempfile::tempdir()?)
}

pub(super) struct Seeded {
    pub project_id: i64,
    pub session_id: i64,
    pub session_uuid: Uuid,
}

pub(super) async fn seed_session(
    handle: &PersistHandle,
    _label: &str,
    cwd: &Path,
) -> anyhow::Result<Seeded> {
    let project_id = match handle.find_project_id_by_path(test_seed::CHECKOUT).await? {
        Some(id) => id,
        None => test_seed::project(handle.pool()).await?,
    };
    let cwd_text = cwd.to_string_lossy().into_owned();
    let session_id = test_seed::session(handle.pool(), project_id, Some(&cwd_text)).await?;
    let identities = handle.session_identities(&[session_id]).await?;
    let identity = identities
        .first()
        .ok_or_else(|| anyhow::anyhow!("seeded session {session_id} has no identity"))?;
    Ok(Seeded {
        project_id,
        session_id,
        session_uuid: identity.uuid,
    })
}

pub(super) async fn events_in(
    handle: &PersistHandle,
    checkout: &str,
) -> anyhow::Result<Vec<ChangeEvent>> {
    Ok(handle
        .change_events_after(&ChangeQuery::everything_after(checkout, 0, 100))
        .await?)
}
