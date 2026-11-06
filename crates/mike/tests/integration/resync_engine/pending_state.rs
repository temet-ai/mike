use crate::test_db;

use super::seed::seed_source;
use super::support::{BASELINE_TABLES, baseline_cursors, connect};
use mike::resync::ResyncEngine;
use test_db::TestDb;

#[tokio::test]
async fn has_pending_true_before_replay_false_after() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping resync_engine has_pending test");
        return Ok(());
    };

    let source_handle = test_db.acquire_isolated().await?;
    let dest_handle = test_db.acquire_isolated().await?;
    let source = connect(&source_handle.database_url).await?;
    let dest = connect(&dest_handle.database_url).await?;

    baseline_cursors(&source, BASELINE_TABLES).await?;
    seed_source(&source).await?;

    let engine = ResyncEngine::new(source.clone(), dest.clone());
    assert!(
        engine.has_pending().await?,
        "seeded rows past cursor=0 must report pending"
    );

    engine.replay_pending().await?;
    assert!(
        !engine.has_pending().await?,
        "a fully-drained source must report no pending rows"
    );

    sqlx::query("INSERT INTO projects.projects (project_path, display_name) VALUES ('/home/user/late-arrival', 'late')")
        .execute(&source)
        .await?;
    assert!(
        engine.has_pending().await?,
        "a row arriving after the drain must report pending again"
    );

    source.close().await;
    dest.close().await;
    source_handle.cleanup().await?;
    dest_handle.cleanup().await?;
    Ok(())
}
