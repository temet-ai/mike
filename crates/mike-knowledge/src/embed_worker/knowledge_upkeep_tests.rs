use chrono::{DateTime, Utc};
use std::time::{Duration, Instant};

use super::knowledge::{SweepUpkeep, lead_sweep};
use super::knowledge_tests::{breaker, embedding_server};
use crate::store::persistence::{PersistHandle, test_support::acquire_ephemeral_db};

#[tokio::test]
async fn lead_sweep_skipped_tick_keeps_the_daily_reconcile_clock() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (_server, service) = embedding_server().await;
    let lease = Duration::from_secs(30);
    let mut upkeep = SweepUpkeep::default();
    assert!(lead_sweep(&handle, &service, &mut breaker(), &mut upkeep, lease).await?);
    let Some(first) = upkeep.reconciled_at() else {
        anyhow::bail!("the first led sweep did not reconcile the knowledge items");
    };
    let mut open = breaker();
    for _ in 0..3 {
        open.record(false, Instant::now());
    }
    assert!(!lead_sweep(&handle, &service, &mut open, &mut upkeep, lease).await?);
    assert_eq!(upkeep.reconciled_at(), Some(first));
    assert!(lead_sweep(&handle, &service, &mut breaker(), &mut upkeep, lease).await?);
    assert_eq!(upkeep.reconciled_at(), Some(first));
    db.cleanup().await
}

#[tokio::test]
async fn lead_sweep_after_a_restart_or_lease_move_keeps_the_daily_reconcile() -> anyhow::Result<()>
{
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (_server, service) = embedding_server().await;
    let lease = Duration::from_secs(30);
    assert!(
        lead_sweep(
            &handle,
            &service,
            &mut breaker(),
            &mut SweepUpkeep::default(),
            lease
        )
        .await?
    );
    let Some(first) = reconciled_at(&handle).await? else {
        anyhow::bail!("the first led sweep did not record its reconcile");
    };
    let mut restarted = SweepUpkeep::default();
    assert!(lead_sweep(&handle, &service, &mut breaker(), &mut restarted, lease).await?);
    assert_eq!(
        reconciled_at(&handle).await?,
        Some(first),
        "a new process reconciled again within the day"
    );
    assert!(restarted.reconciled_at().is_some());
    sqlx::query("UPDATE memory.knowledge_upkeep SET reconciled_at = now() - interval '25 hours'")
        .execute(handle.pool())
        .await?;
    assert!(
        lead_sweep(
            &handle,
            &service,
            &mut breaker(),
            &mut SweepUpkeep::default(),
            lease
        )
        .await?
    );
    assert!(reconciled_at(&handle).await?.is_some_and(|at| at > first));
    db.cleanup().await
}

async fn reconciled_at(handle: &PersistHandle) -> anyhow::Result<Option<DateTime<Utc>>> {
    Ok(
        sqlx::query_scalar("SELECT reconciled_at FROM memory.knowledge_upkeep WHERE id = 1")
            .fetch_optional(handle.pool())
            .await?,
    )
}
