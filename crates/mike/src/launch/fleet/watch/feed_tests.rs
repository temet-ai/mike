use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::{Tree, git};
use crate::change_record::watch::{Checkout, WatchConfig, start, subscribe};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::NewTreeChange;
use crate::store::persistence::test_support::{acquire_ephemeral_db, seed_project};

async fn run(repo: &str, args: &[&str]) -> anyhow::Result<()> {
    git(repo, args, None)
        .await
        .map(|_| ())
        .ok_or_else(|| anyhow::anyhow!("git {args:?}"))
}

fn rows(changes: &[NewTreeChange]) -> Vec<(String, String, Option<String>)> {
    let mut out: Vec<_> = changes
        .iter()
        .map(|c| (c.path.clone(), c.change.clone(), c.content_sha.clone()))
        .collect();
    out.sort();
    out
}

#[tokio::test]
async fn rows_taken_from_the_record_feed_equal_the_rows_the_git_poll_gives() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let dir = tempfile::tempdir()?;
    let repo = std::fs::canonicalize(dir.path())?
        .to_string_lossy()
        .into_owned();
    run(&repo, &["init", "-q", "-b", "main"]).await?;
    run(&repo, &["config", "user.email", "t@example.com"]).await?;
    run(&repo, &["config", "user.name", "t"]).await?;
    std::fs::write(dir.path().join("a.txt"), "one")?;
    std::fs::write(dir.path().join("b.txt"), "two")?;
    run(&repo, &["add", "."]).await?;
    run(&repo, &["commit", "-q", "-m", "base"]).await?;

    let persist = PersistHandle::connect(&db.database_url).await?;
    let project_id = seed_project(persist.pool(), &repo).await?;
    let config = WatchConfig {
        debounce: Duration::from_millis(100),
        backstop: Duration::from_secs(3600),
        ..WatchConfig::default()
    };
    let running = start(
        persist,
        Checkout {
            root: repo.clone(),
            project_id,
        },
        config,
        CancellationToken::new(),
    );
    for _ in 0..200 {
        if running
            .stats
            .ready
            .load(std::sync::atomic::Ordering::Acquire)
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let mut feed = subscribe(&repo).ok_or_else(|| anyhow::anyhow!("no feed"))?;
    let mut fed = Tree::baseline(&repo)
        .await
        .ok_or_else(|| anyhow::anyhow!("baseline"))?;
    let mut polled = Tree::baseline(&repo)
        .await
        .ok_or_else(|| anyhow::anyhow!("baseline"))?;

    std::fs::write(dir.path().join("a.txt"), "changed")?;
    std::fs::write(dir.path().join("c.txt"), "new")?;
    std::fs::remove_file(dir.path().join("b.txt"))?;

    let mut from_feed = Vec::new();
    while from_feed.len() < 3 {
        let deltas = tokio::time::timeout(Duration::from_secs(10), feed.recv()).await??;
        from_feed.extend(fed.apply_feed(&deltas));
    }
    let from_poll = polled.observe(&repo).await;

    running.token.cancel();
    let _ = running.handle.await;
    db.cleanup().await?;

    assert_eq!(rows(&from_feed), rows(&from_poll));
    assert_eq!(fed.known, polled.known);
    Ok(())
}
