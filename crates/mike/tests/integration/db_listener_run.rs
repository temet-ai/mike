use std::time::Duration;

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::test_db::TestDb;

use mike::db_listener::run;
use mike::store::models::{ProxyEvent, ResyncReason, RowChangedOp, RowChangedTable};

fn assert_resync_required(event: ProxyEvent, reason: ResyncReason) -> anyhow::Result<()> {
    match event {
        ProxyEvent::ResyncRequired { reason: r } => {
            assert_eq!(r, reason);
            Ok(())
        }
        other => anyhow::bail!("unexpected variant: {other:?}"),
    }
}

async fn recv_consolidation_runs_insert(
    rx: &mut broadcast::Receiver<ProxyEvent>,
    id: i64,
) -> anyhow::Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        anyhow::ensure!(
            !remaining.is_zero(),
            "timed out waiting for consolidation_runs RowChanged"
        );
        let event = tokio::time::timeout(remaining, rx.recv())
            .await
            .map_err(|_| {
                anyhow::anyhow!("timed out waiting for consolidation_runs RowChanged")
            })??;
        if let ProxyEvent::RowChanged {
            table, id: eid, op, ..
        } = &event
            && *table == RowChangedTable::ConsolidationRuns
            && eid == &id.to_string()
        {
            assert_eq!(*op, RowChangedOp::Insert);
            return Ok(());
        }
    }
}

#[tokio::test]
async fn run_establishes_listener_and_forwards_consolidation_runs_insert() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping db_listener run() establishment test");
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let (tx, mut rx) = broadcast::channel(16);
    let shutdown = CancellationToken::new();
    let task_shutdown = shutdown.clone();
    let join = tokio::spawn(run(handle.database_url.clone(), tx, task_shutdown));

    let first = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .map_err(|_| anyhow::anyhow!("timed out waiting for ListenerEstablishment"))??;
    assert_resync_required(first, ResyncReason::ListenerEstablishment)?;

    let run_id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.consolidation_runs (run_kind_id, trigger_id, status_id, started_at)
         SELECT k.id, t.id, s.id, now()
           FROM memory.consolidation_run_kinds k,
                memory.consolidation_triggers t,
                memory.consolidation_run_statuses s
          WHERE k.code = 'session' AND t.code = 'manual' AND s.code = 'running'
         RETURNING id",
    )
    .fetch_one(&pool)
    .await?;

    recv_consolidation_runs_insert(&mut rx, run_id).await?;

    shutdown.cancel();
    let join_result = tokio::time::timeout(Duration::from_secs(5), join)
        .await
        .map_err(|_| anyhow::anyhow!("run() did not exit within 5s of shutdown"))?;
    join_result??;

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}
