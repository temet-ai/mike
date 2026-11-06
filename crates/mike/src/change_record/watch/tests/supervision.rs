use chrono::{Duration, Utc};
use tokio_util::sync::CancellationToken;

use super::{db, quick, repo};
use crate::change_record::watch::is_watching;
use crate::change_record::watch::supervisor::Supervisor;
use crate::store::persistence::change_record::test_seed::{request, session};

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
    let mut supervisor = Supervisor::new(db.persist.clone(), quick(true));
    supervisor.tick(&shutdown).await;
    let watched: Vec<String> = supervisor.watching().into_iter().collect();
    assert_eq!(watched, vec![busy.root.clone()]);
    assert!(!is_watching(&quiet.root));

    shutdown.cancel();
    db.db.cleanup().await
}
