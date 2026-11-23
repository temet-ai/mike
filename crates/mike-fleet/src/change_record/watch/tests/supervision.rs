use chrono::{Duration, Utc};
use tokio_util::sync::CancellationToken;

use super::{db, quick, repo};
use crate::change_feed::ChangeFeed;
use crate::change_record::watch::is_watching;
use crate::change_record::watch::supervisor::Supervisor;
use crate::store::persistence::change_record::test_seed::{micros_now, request, session};

#[tokio::test]
async fn a_checkout_with_a_working_session_is_watched_and_a_quiet_one_is_not() -> anyhow::Result<()>
{
    let busy = repo().await?;
    let quiet = repo().await?;
    let Some(db) = db(&busy.root).await? else {
        return Ok(());
    };
    let pool = db.persist.pool().clone();
    let inside = busy.git.absolute("src").to_string_lossy().into_owned();
    let worker = session(&pool, db.project_id, Some(&inside)).await?;
    request(&pool, worker, db.project_id, Utc::now()).await?;
    let idle = session(&pool, db.project_id, Some(&quiet.root)).await?;
    request(&pool, idle, db.project_id, Utc::now() - Duration::hours(2)).await?;
    let gone = session(
        &pool,
        db.project_id,
        Some("/no/such/folder/on/this/machine"),
    )
    .await?;
    request(&pool, gone, db.project_id, Utc::now()).await?;

    let shutdown = CancellationToken::new();
    let mut supervisor = Supervisor::new(db.persist.clone(), ChangeFeed::new(), quick(true));
    supervisor.tick(&shutdown).await;
    let watched: Vec<String> = supervisor.watching().into_iter().collect();
    assert_eq!(watched, vec![busy.root.clone()]);
    assert!(!is_watching(&quiet.root));

    shutdown.cancel();
    db.db.cleanup().await
}

#[tokio::test]
async fn a_watched_checkout_removed_from_disk_is_no_longer_watched() -> anyhow::Result<()> {
    let busy = repo().await?;
    let Some(db) = db(&busy.root).await? else {
        return Ok(());
    };
    let pool = db.persist.pool().clone();
    let worker = session(&pool, db.project_id, Some(&busy.root)).await?;
    request(&pool, worker, db.project_id, Utc::now()).await?;

    let shutdown = CancellationToken::new();
    let mut supervisor = Supervisor::new(db.persist.clone(), ChangeFeed::new(), quick(true));
    supervisor.tick(&shutdown).await;
    assert_eq!(supervisor.watching().len(), 1);
    std::fs::remove_dir_all(&busy.root)?;
    supervisor.tick(&shutdown).await;
    assert!(supervisor.watching().is_empty());

    shutdown.cancel();
    db.db.cleanup().await
}

#[tokio::test]
async fn a_watched_checkout_is_read_again_when_its_newest_request_turns_thirty_minutes_old()
-> anyhow::Result<()> {
    let busy = repo().await?;
    let Some(db) = db(&busy.root).await? else {
        return Ok(());
    };
    let pool = db.persist.pool().clone();
    let newest = micros_now()? - Duration::minutes(29);
    let worker = session(&pool, db.project_id, Some(&busy.root)).await?;
    request(&pool, worker, db.project_id, newest - Duration::minutes(5)).await?;
    request(&pool, worker, db.project_id, newest).await?;
    let elsewhere = session(
        &pool,
        db.project_id,
        Some("/no/such/folder/on/this/machine"),
    )
    .await?;
    request(
        &pool,
        elsewhere,
        db.project_id,
        newest - Duration::minutes(20),
    )
    .await?;

    let shutdown = CancellationToken::new();
    let mut supervisor = Supervisor::new(db.persist.clone(), ChangeFeed::new(), quick(true));
    supervisor.tick(&shutdown).await;
    assert_eq!(supervisor.watching().len(), 1);
    assert_eq!(
        supervisor.read_again_at(),
        Some(newest + Duration::minutes(30) + Duration::seconds(1))
    );

    shutdown.cancel();
    db.db.cleanup().await
}

#[tokio::test]
async fn a_supervisor_with_nothing_watched_has_no_read_due() -> anyhow::Result<()> {
    let quiet = repo().await?;
    let Some(db) = db(&quiet.root).await? else {
        return Ok(());
    };
    let pool = db.persist.pool().clone();
    let idle = session(&pool, db.project_id, Some(&quiet.root)).await?;
    request(&pool, idle, db.project_id, Utc::now() - Duration::hours(2)).await?;

    let shutdown = CancellationToken::new();
    let mut supervisor = Supervisor::new(db.persist.clone(), ChangeFeed::new(), quick(true));
    supervisor.tick(&shutdown).await;
    assert!(supervisor.watching().is_empty());
    assert_eq!(supervisor.read_again_at(), None);

    shutdown.cancel();
    db.db.cleanup().await
}
