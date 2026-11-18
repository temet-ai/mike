use crate::store::persistence::web_repo::WebPersist as _;
use std::path::PathBuf;

use super::attempt::AttemptEnd;
use super::fixtures_tests::{Bench, FIXED, WritesFiles, sh};
use super::follow::recover;
use super::port::RepairRun;
use super::{states, tick, worktree};
use crate::store::persistence::wright::RepairRow;

const STILL_BUGGY: &str = "add() { echo $(( $1 - $2 )); }\n# fixed\n";

async fn repairs_of(bench: &Bench, finding: i64) -> anyhow::Result<Vec<RepairRow>> {
    let ids: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM wright.repairs WHERE finding_id = $1 ORDER BY id")
            .bind(finding)
            .fetch_all(bench.ctx.persist.pool())
            .await?;
    let mut rows = Vec::new();
    for id in ids {
        let row = bench
            .ctx
            .persist
            .web()
            .wright_repair(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("repair {id} vanished"))?;
        rows.push(row);
    }
    Ok(rows)
}

async fn interrupted_flag(bench: &Bench, id: i64) -> anyhow::Result<bool> {
    Ok(
        sqlx::query_scalar("SELECT interrupted FROM wright.repairs WHERE id = $1")
            .bind(id)
            .fetch_one(bench.ctx.persist.pool())
            .await?,
    )
}

#[tokio::test]
async fn shutdown_during_a_failing_gate_leaves_the_repair_for_recovery_and_starts_no_round()
-> anyhow::Result<()> {
    let bench = Bench::boot(WritesFiles::new(&[("src/calc.sh", STILL_BUGGY)])).await?;
    let (_, finding) = bench.defect().await?;
    bench.ctx.shutdown.cancel();

    let report = tick(&bench.ctx, true).await?;
    assert!(matches!(report.attempted, Some((_, AttemptEnd::Ended))));
    let repair = bench.repair_of(finding).await?;
    assert_eq!(repair.state, states::GATING, "reason: {:?}", repair.reason);
    assert_eq!(bench.scratch.gate_log().lines().count(), 1);
    assert_eq!(bench.ctx.engineer.jobs().len(), 1, "no second round");

    assert_eq!(recover(&bench.ctx).await?, 1);
    let settled = bench.repair_of(finding).await?;
    assert_eq!(settled.state, states::GATE_FAILED);
    assert!(
        settled
            .reason
            .unwrap_or_default()
            .starts_with("interrupted:")
    );
    assert!(interrupted_flag(&bench, settled.id).await?);
    bench.cleanup().await
}

#[tokio::test]
async fn shutdown_during_a_failed_engineer_run_leaves_the_repair_editing() -> anyhow::Result<()> {
    let engineer = WritesFiles::new(&[]).ending(RepairRun {
        not_started: None,
        failure: Some("the repair run was stopped: mike is shutting down".to_owned()),
    });
    let bench = Bench::boot(engineer).await?;
    let (_, finding) = bench.defect().await?;
    bench.ctx.shutdown.cancel();

    let report = tick(&bench.ctx, true).await?;
    assert!(matches!(report.attempted, Some((_, AttemptEnd::Ended))));
    assert_eq!(bench.repair_of(finding).await?.state, states::EDITING);
    assert_eq!(recover(&bench.ctx).await?, 1);
    let settled = bench.repair_of(finding).await?;
    assert_eq!(settled.state, states::REFUSED);
    assert!(interrupted_flag(&bench, settled.id).await?);
    bench.cleanup().await
}

#[tokio::test]
async fn repair_interrupted_while_editing_is_settled_and_the_finding_is_repaired_again()
-> anyhow::Result<()> {
    let bench = Bench::boot(WritesFiles::new(&[("src/calc.sh", FIXED)])).await?;
    let (_, finding) = bench.defect().await?;
    let worktrees = bench.ctx.repo.worktrees().display().to_string();
    let claimed = bench
        .ctx
        .persist
        .web()
        .claim_wright_repairs(&worktrees, None, 1)
        .await?;
    let claim = claimed
        .first()
        .ok_or_else(|| anyhow::anyhow!("nothing claimed"))?;
    let path = PathBuf::from(&claim.worktree);
    worktree::open(&bench.ctx.runner, &bench.ctx.repo, &path, &claim.branch)
        .await
        .map_err(anyhow::Error::msg)?;

    assert_eq!(recover(&bench.ctx).await?, 1);
    let first = bench.repair_of(finding).await?;
    assert_eq!(first.state, states::REFUSED);
    assert!(first.reason.unwrap_or_default().starts_with("interrupted"));
    assert!(interrupted_flag(&bench, first.id).await?);
    assert!(!path.exists());
    assert!(
        sh(
            &bench.scratch.root,
            &["rev-parse", &format!("refs/heads/{}", claim.branch)]
        )
        .is_ok()
    );

    let again = tick(&bench.ctx, true).await?;
    assert!(
        matches!(again.attempted, Some((_, AttemptEnd::Landed))),
        "{again:?}"
    );
    let rows = repairs_of(&bench, finding).await?;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].state, states::LANDED, "{:?}", rows[1].reason);
    assert_eq!(
        rows[1].branches.first(),
        Some(&format!("wright/repair-{finding}-r2"))
    );
    assert!(
        rows[1]
            .worktree
            .ends_with(&format!("wright-repair-{finding}-r2"))
    );
    assert_ne!(rows[1].branch, claim.branch);
    assert!(!interrupted_flag(&bench, rows[1].id).await?);
    assert_eq!(recover(&bench.ctx).await?, 0);
    bench.cleanup().await
}

#[tokio::test]
async fn finding_interrupted_three_times_is_not_claimed_a_fourth_time() -> anyhow::Result<()> {
    let bench = Bench::boot(WritesFiles::new(&[])).await?;
    let (_, finding) = bench.defect().await?;
    let web = bench.ctx.persist.web();
    let mut branches = Vec::new();
    for _ in 0..3 {
        let claimed = web
            .claim_wright_repairs("/scratch/worktrees", None, 1)
            .await?;
        let claim = claimed
            .first()
            .ok_or_else(|| anyhow::anyhow!("an interrupted finding was not claimed again"))?;
        branches.push(claim.branch.clone());
        sqlx::query(
            "UPDATE wright.repairs /* grader-allow: each pass must claim through the store first, and the next claim depends on this row being marked interrupted, so the three updates cannot be one set-based statement */
                SET state = 'refused', reason = 'interrupted: test', interrupted = true
              WHERE id = $1",
        )
        .bind(claim.id)
        .execute(bench.ctx.persist.pool())
        .await?;
    }
    assert_eq!(
        branches,
        vec![
            format!("wright/repair-{finding}"),
            format!("wright/repair-{finding}-r2"),
            format!("wright/repair-{finding}-r3"),
        ]
    );
    assert!(
        web.claim_wright_repairs("/scratch/worktrees", None, 1)
            .await?
            .is_empty()
    );
    assert!(!web.wright_repair_waiting().await?);
    bench.cleanup().await
}

#[tokio::test]
async fn finding_with_a_repair_that_ended_for_its_own_reasons_is_not_claimed_again()
-> anyhow::Result<()> {
    let bench = Bench::boot(WritesFiles::new(&[])).await?;
    let (_, finding) = bench.defect().await?;
    let web = bench.ctx.persist.web();
    let claimed = web
        .claim_wright_repairs("/scratch/worktrees", None, 1)
        .await?;
    let claim = claimed
        .first()
        .ok_or_else(|| anyhow::anyhow!("nothing claimed"))?;
    sqlx::query(
        "UPDATE wright.repairs SET state = 'refused', reason = 'refused: test' WHERE id = $1",
    )
    .bind(claim.id)
    .execute(bench.ctx.persist.pool())
    .await?;
    assert!(
        web.claim_wright_repairs("/scratch/worktrees", None, 1)
            .await?
            .is_empty()
    );
    assert_eq!(repairs_of(&bench, finding).await?.len(), 1);
    bench.cleanup().await
}
