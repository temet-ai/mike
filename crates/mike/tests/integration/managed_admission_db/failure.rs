use mike::launch::fleet::test_seams::{control::Phase, durable};
use mike::store::persistence::fleet_repo::FleetPersist as _;

use super::support::Harness;

#[tokio::test]
async fn wrong_exact_run_and_absent_fleet_cannot_insert_a_receipt() -> anyhow::Result<()> {
    let Some(h) = Harness::new().await? else {
        return Ok(());
    };
    let mut wrong_run = h.request("touch must-not-run");
    wrong_run.run.push_str("-different");
    assert!(h.domain.begin(h.persist.clone(), wrong_run).await.is_err());
    let mut absent = h.request("touch must-not-run");
    absent.fleet = uuid::Uuid::new_v4();
    assert!(h.domain.begin(h.persist.clone(), absent).await.is_err());
    assert!(
        h.persist
            .fleet()
            .pending_fleet_commands(&h.run)
            .await?
            .is_empty()
    );
    assert!(!h.repo.path().join("must-not-run").exists());
    h.finish().await
}

#[tokio::test]
async fn deferred_commit_rejection_retains_unknown_outcome_entry_without_inventing_a_receipt()
-> anyhow::Result<()> {
    let Some(h) = Harness::new().await? else {
        return Ok(());
    };
    sqlx::query("CREATE FUNCTION launch.reject_fixture_receipt_commit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture deferred receipt commit rejected'; END; $$")
        .execute(h.other.pool()).await?;
    sqlx::query("CREATE CONSTRAINT TRIGGER reject_fixture_receipt_commit AFTER INSERT ON launch.tree_checks DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION launch.reject_fixture_receipt_commit()")
        .execute(h.other.pool()).await?;
    let answer = h
        .domain
        .begin(h.persist.clone(), h.request("touch must-not-run"))
        .await;
    let why = match answer {
        Ok(_) => anyhow::bail!("fixture deferred commit unexpectedly succeeded"),
        Err(why) => why,
    };
    assert!(
        why.contains("fixture deferred receipt commit rejected"),
        "{why}"
    );
    h.stop_other().await?;
    durable::local_stop_fence(&h.run).await.stopping_persisted();
    let snapshot = durable::snapshot(&h.run);
    assert_eq!(snapshot.pending.len(), 1);
    assert_eq!(snapshot.pending[0].phase, Phase::ReceiptBlocked);
    assert!(snapshot.pending[0].receipt.is_some());
    assert!(
        snapshot.pending[0]
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("commit outcome unknown"))
    );
    assert_eq!(snapshot.pending[0].owned_processes, 0);
    assert!(!snapshot.locally_settled());
    assert!(
        h.other
            .fleet()
            .pending_fleet_commands(&h.run)
            .await?
            .is_empty()
    );
    assert!(
        h.other
            .fleet()
            .latest_tree_check(&h.request("").ledger)
            .await?
            .is_none()
    );
    assert!(!h.repo.path().join("must-not-run").exists());
    h.finish().await
}
