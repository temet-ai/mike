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
