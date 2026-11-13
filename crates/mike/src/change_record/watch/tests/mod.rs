use crate::store::persistence::fleet_repo::FleetPersist as _;
mod authors;
mod drill;
mod feed;
mod idle;
mod proof;
mod pure;
mod pushes;
mod refs_moves;
mod supervision;

use std::sync::atomic::Ordering;
use std::time::Duration;

use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

use super::{Checkout, Git, Running, WatchConfig, start};
use crate::change_record::{ChangeEvent, ChangeKind, ChangeQuery};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db, seed_project};

pub(super) struct Repo {
    _dir: TempDir,
    pub root: String,
    pub git: Git,
}

impl Repo {
    pub fn write(&self, path: &str, text: &str) -> anyhow::Result<()> {
        let full = self.git.absolute(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(full, text)?;
        Ok(())
    }

    pub async fn git(&self, args: &[&str]) -> anyhow::Result<String> {
        let raw = self.git.run(args).await?;
        Ok(String::from_utf8_lossy(&raw).trim().to_string())
    }
}

pub(super) async fn repo() -> anyhow::Result<Repo> {
    let dir = tempfile::tempdir()?;
    let root = std::fs::canonicalize(dir.path())?
        .to_string_lossy()
        .into_owned();
    let git = Git::new(root.clone());
    let repo = Repo {
        _dir: dir,
        root,
        git,
    };
    repo.git(&["init", "-q", "-b", "main"]).await?;
    repo.git(&["config", "user.email", "t@example.com"]).await?;
    repo.git(&["config", "user.name", "t"]).await?;
    repo.write(".gitignore", "target/\nnode_modules/\n")?;
    repo.write("src/a.txt", "one")?;
    repo.git(&["add", "."]).await?;
    repo.git(&["commit", "-q", "-m", "base"]).await?;
    Ok(repo)
}

pub(super) struct Db {
    pub db: TestDbHandle,
    pub persist: PersistHandle,
    pub project_id: i64,
}

pub(super) async fn db(root: &str) -> anyhow::Result<Option<Db>> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(None);
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let project_id = seed_project(persist.pool(), root).await?;
    Ok(Some(Db {
        db,
        persist,
        project_id,
    }))
}

pub(super) fn quick(notifications: bool) -> WatchConfig {
    WatchConfig {
        notifications,
        debounce: Duration::from_millis(100),
        backstop: Duration::from_secs(3600),
        fallback_poll: Duration::from_millis(400),
        author_delay: Duration::from_millis(200),
        edit_window: Duration::from_secs(30),
        hold_retries: false,
    }
}

pub(super) async fn watching(db: &Db, repo: &Repo, config: WatchConfig) -> anyhow::Result<Running> {
    let running = start(
        db.persist.clone(),
        Checkout {
            root: repo.root.clone(),
            project_id: db.project_id,
        },
        config,
        CancellationToken::new(),
    );
    for _ in 0..200 {
        if running.stats.ready.load(Ordering::Acquire) {
            return Ok(running);
        }
        anyhow::ensure!(
            !running.handle.is_finished(),
            "the watcher of {} ended before it was ready",
            repo.root
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    anyhow::bail!("the watcher of {} was not ready within 10 s", repo.root)
}

pub(super) async fn events(db: &Db, root: &str) -> anyhow::Result<Vec<ChangeEvent>> {
    Ok(db
        .persist
        .fleet()
        .change_events_after(&ChangeQuery::everything_after(root, 0, 1000))
        .await?)
}

pub(super) async fn wait_for<F>(
    db: &Db,
    root: &str,
    what: &str,
    done: F,
) -> anyhow::Result<Vec<ChangeEvent>>
where
    F: Fn(&[ChangeEvent]) -> bool,
{
    let mut seen = Vec::new();
    for _ in 0..300 {
        seen = events(db, root).await?;
        if done(&seen) {
            return Ok(seen);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let shown: Vec<(ChangeKind, Vec<String>, serde_json::Value)> = seen
        .iter()
        .map(|e| (e.kind, e.paths.clone(), e.detail.clone()))
        .collect();
    anyhow::bail!("waited 15 s for {what}; the record holds {shown:?}")
}

pub(super) fn with_path<'a>(events: &'a [ChangeEvent], path: &str) -> Vec<&'a ChangeEvent> {
    events
        .iter()
        .filter(|e| e.paths.iter().any(|p| p == path))
        .collect()
}

pub(super) async fn stop(running: Running, db: Db) -> anyhow::Result<()> {
    running.token.cancel();
    tokio::time::timeout(Duration::from_secs(10), running.handle).await??;
    db.db.cleanup().await
}
