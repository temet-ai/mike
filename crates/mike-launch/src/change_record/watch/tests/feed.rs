use std::sync::atomic::Ordering;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::{db, events, quick, repo, stop, wait_for, watching, with_path};
use crate::change_record::watch::{Checkout, DeltaKind, Git, is_watching, start, subscribe};

#[tokio::test]
async fn a_written_file_is_recorded_from_notifications_without_a_poll() -> anyhow::Result<()> {
    let repo = repo().await?;
    let Some(db) = db(&repo.root).await? else {
        return Ok(());
    };
    let before = repo.git(&["rev-parse", "HEAD:src/a.txt"]).await?;
    let running = watching(&db, &repo, quick(true)).await?;
    assert!(running.stats.notifications.load(Ordering::Acquire));
    repo.write("src/a.txt", "two")?;
    repo.write("src/new.txt", "fresh")?;
    let seen = wait_for(&db, &repo.root, "both writes", |e| e.len() >= 2).await?;
    let changed = with_path(&seen, "src/a.txt");
    assert_eq!(changed.len(), 1, "one row for the changed file: {seen:?}");
    assert_eq!(changed[0].before_sha.as_deref(), Some(before.as_str()));
    let after = changed[0].after_sha.clone().unwrap_or_default();
    assert_eq!(repo.git(&["cat-file", "-p", &after]).await?, "two");
    assert_eq!(repo.git(&["cat-file", "-p", &before]).await?, "one");
    let added = with_path(&seen, "src/new.txt");
    assert_eq!(added.len(), 1);
    assert_eq!(added[0].before_sha, None);
    assert_eq!(added[0].detail["seen_by"], "notification");
    assert_eq!(
        running.stats.backstop_polls.load(Ordering::Acquire),
        0,
        "the record came from notifications, not a status poll"
    );
    stop(running, db).await
}

#[tokio::test]
async fn with_notifications_off_the_backstop_poll_records_the_write() -> anyhow::Result<()> {
    let repo = repo().await?;
    let Some(db) = db(&repo.root).await? else {
        return Ok(());
    };
    let running = watching(&db, &repo, quick(false)).await?;
    assert!(!running.stats.notifications.load(Ordering::Acquire));
    repo.write("src/polled.txt", "seen by the poll")?;
    let seen = wait_for(&db, &repo.root, "the polled write", |e| {
        !with_path(e, "src/polled.txt").is_empty()
    })
    .await?;
    assert_eq!(
        with_path(&seen, "src/polled.txt")[0].detail["seen_by"],
        "backstop"
    );
    assert!(running.stats.backstop_polls.load(Ordering::Acquire) >= 1);
    stop(running, db).await
}

#[tokio::test]
async fn ignored_folders_and_git_objects_yield_no_rows() -> anyhow::Result<()> {
    let repo = repo().await?;
    let Some(db) = db(&repo.root).await? else {
        return Ok(());
    };
    let running = watching(&db, &repo, quick(true)).await?;
    repo.write("target/debug/build.out", "x")?;
    repo.write("node_modules/pkg/index.js", "x")?;
    repo.write(".git/objects/zz/not-an-object", "x")?;
    tokio::time::sleep(Duration::from_millis(400)).await;
    repo.write("target/debug/build.out", "y")?;
    repo.write("node_modules/pkg/index.js", "y")?;
    repo.write("src/marker.txt", "after the ignored writes")?;
    let seen = wait_for(&db, &repo.root, "the marker", |e| {
        !with_path(e, "src/marker.txt").is_empty()
    })
    .await?;
    let paths: Vec<&Vec<String>> = seen.iter().map(|e| &e.paths).collect();
    assert_eq!(paths, vec![&vec!["src/marker.txt".to_string()]]);
    stop(running, db).await
}

#[tokio::test]
async fn a_new_folder_is_watched_and_a_deleted_file_keeps_its_content_before() -> anyhow::Result<()>
{
    let repo = repo().await?;
    let Some(db) = db(&repo.root).await? else {
        return Ok(());
    };
    let running = watching(&db, &repo, quick(true)).await?;
    repo.write("src/deep/er/f.txt", "first")?;
    wait_for(&db, &repo.root, "the file in the new folder", |e| {
        with_path(e, "src/deep/er/f.txt").len() == 1
    })
    .await?;
    tokio::time::sleep(Duration::from_millis(300)).await;
    repo.write("src/deep/er/f.txt", "second")?;
    std::fs::remove_file(repo.git.absolute("src/a.txt"))?;
    let seen = wait_for(&db, &repo.root, "the second write and the delete", |e| {
        with_path(e, "src/deep/er/f.txt").len() == 2 && !with_path(e, "src/a.txt").is_empty()
    })
    .await?;
    let gone = with_path(&seen, "src/a.txt");
    assert_eq!(gone[0].detail["change"], "deleted");
    assert!(gone[0].before_sha.is_some());
    assert_eq!(gone[0].after_sha, None);
    stop(running, db).await
}

#[tokio::test]
async fn the_fleet_feed_carries_each_change() -> anyhow::Result<()> {
    let repo = repo().await?;
    let Some(db) = db(&repo.root).await? else {
        return Ok(());
    };
    let running = watching(&db, &repo, quick(true)).await?;
    assert!(is_watching(&format!("{}/", repo.root)));
    let mut feed = subscribe(&repo.root).ok_or_else(|| anyhow::anyhow!("no feed"))?;
    repo.write("src/a.txt", "fed")?;
    let deltas = tokio::time::timeout(Duration::from_secs(10), feed.recv()).await??;
    assert_eq!(deltas.len(), 1);
    assert_eq!(deltas[0].path, "src/a.txt");
    assert_eq!(deltas[0].change, DeltaKind::Modified);
    let root = repo.root.clone();
    stop(running, db).await?;
    for _ in 0..100 {
        if !is_watching(&root) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    anyhow::bail!("the feed of {root} stayed registered after the watcher stopped")
}

#[tokio::test]
async fn a_folder_that_is_not_a_repository_is_not_watched() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let root = dir.path().to_string_lossy().into_owned();
    assert!(Git::new(root.clone()).show_toplevel().await.is_err());
    let Some(db) = db(&root).await? else {
        return Ok(());
    };
    let running = start(
        db.persist.clone(),
        Checkout {
            root: root.clone(),
            project_id: db.project_id,
        },
        quick(true),
        CancellationToken::new(),
    );
    tokio::time::timeout(Duration::from_secs(10), running.handle).await??;
    assert!(!running.stats.ready.load(Ordering::Acquire));
    assert!(!is_watching(&root));
    assert!(events(&db, &root).await?.is_empty());
    db.db.cleanup().await
}

#[tokio::test]
async fn hashing_skips_a_path_that_vanished_and_keeps_the_rest() -> anyhow::Result<()> {
    let repo = repo().await?;
    let hashed = repo
        .git
        .hash_paths(&["src/a.txt".to_string(), "src/missing.txt".to_string()])
        .await?;
    assert_eq!(hashed.len(), 1);
    assert_eq!(hashed[0].0, "src/a.txt");
    Ok(())
}
