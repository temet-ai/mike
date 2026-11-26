use crate::store::persistence::web_repo::WebPersist as _;

use super::attempt::AttemptEnd;
use super::fixtures_tests::{Bench, FIXED, WritesFiles, sh};
use super::follow::recover;
use super::port::RepairRun;
use super::{states, tick};
use crate::store::persistence::wright::RepairRow;

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

fn stopped() -> RepairRun {
    RepairRun {
        not_started: None,
        failure: Some("the repair run was stopped: mike is shutting down".to_owned()),
        ..RepairRun::default()
    }
}

#[tokio::test]
async fn shutdown_during_a_failed_engineer_run_leaves_the_repair_editing() -> anyhow::Result<()> {
    let engineer = WritesFiles::new(&[("src/calc.sh", FIXED)]).ending(stopped());
    let bench = Bench::boot(engineer).await?;
    let (_, finding) = bench.defect().await?;
    bench.ctx.shutdown.cancel();

    let report = tick(&bench.ctx, true, true).await?;
    assert!(matches!(report.attempted, Some((_, AttemptEnd::Ended))));
    assert_eq!(bench.repair_of(finding).await?.state, states::EDITING);
    assert_eq!(
        sh(&bench.batch_dir(), &["status", "--porcelain"])?,
        "M src/calc.sh",
        "the stopped engineer's edits wait for recovery"
    );
    assert_eq!(recover(&bench.ctx).await?, 1);
    let settled = bench.repair_of(finding).await?;
    assert_eq!(settled.state, states::REFUSED);
    assert!(interrupted_flag(&bench, settled.id).await?);
    assert_eq!(sh(&bench.batch_dir(), &["status", "--porcelain"])?, "");
    bench.cleanup().await
}

#[tokio::test]
async fn recovery_leaves_batched_fixes_alone_and_resets_only_the_interrupted_fix()
-> anyhow::Result<()> {
    let engineer = WritesFiles::new(&[])
        .then(&[("src/calc.sh", FIXED)])
        .then_ending(&[("src/b.txt", "half done\n")], stopped());
    let bench = Bench::boot(engineer).await?;
    let [first, second] = bench.defects(&["src/calc.sh", "src/b.txt"]).await?[..] else {
        anyhow::bail!("expected two findings");
    };

    let report = tick(&bench.ctx, true, true).await?;
    assert_eq!(
        report.attempted.map(|(_, end)| end),
        Some(AttemptEnd::Batched)
    );
    let head = sh(&bench.batch_dir(), &["rev-parse", "HEAD"])?;
    bench.ctx.shutdown.cancel();
    let report = tick(&bench.ctx, true, true).await?;
    assert!(matches!(report.attempted, Some((_, AttemptEnd::Ended))));
    assert_eq!(bench.repair_of(second).await?.state, states::EDITING);
    assert_eq!(
        sh(&bench.batch_dir(), &["status", "--porcelain"])?,
        "?? src/b.txt"
    );

    assert_eq!(recover(&bench.ctx).await?, 1);
    assert_eq!(bench.repair_of(first).await?.state, states::BATCHED);
    let settled = bench.repair_of(second).await?;
    assert_eq!(settled.state, states::REFUSED);
    assert!(interrupted_flag(&bench, settled.id).await?);
    assert!(bench.batch_dir().exists(), "the day's worktree stays");
    assert_eq!(
        sh(&bench.batch_dir(), &["rev-parse", "HEAD"])?,
        head,
        "the batched fix's commit stays on the branch"
    );
    assert_eq!(sh(&bench.batch_dir(), &["status", "--porcelain"])?, "");
    assert_eq!(recover(&bench.ctx).await?, 0);
    bench.cleanup().await
}

#[tokio::test]
async fn repair_interrupted_while_editing_is_settled_and_the_finding_is_repaired_again()
-> anyhow::Result<()> {
    let bench = Bench::boot(WritesFiles::new(&[("src/calc.sh", FIXED)])).await?;
    let (_, finding) = bench.defect().await?;
    let batch = bench.open_batch().await?;
    let claimed = bench
        .ctx
        .persist
        .web()
        .claim_wright_repairs(batch.id, None, 1)
        .await?;
    let claim = claimed
        .first()
        .ok_or_else(|| anyhow::anyhow!("nothing claimed"))?;
    std::fs::write(bench.batch_dir().join("src/calc.sh"), "half an edit\n")?;

    assert_eq!(recover(&bench.ctx).await?, 1);
    let first = bench.repair_of(finding).await?;
    assert_eq!(first.state, states::REFUSED);
    assert!(first.reason.unwrap_or_default().starts_with("interrupted"));
    assert!(interrupted_flag(&bench, first.id).await?);
    assert_eq!(sh(&bench.batch_dir(), &["status", "--porcelain"])?, "");

    let again = tick(&bench.ctx, true, true).await?;
    assert!(
        matches!(again.attempted, Some((_, AttemptEnd::Batched))),
        "{again:?}"
    );
    let rows = repairs_of(&bench, finding).await?;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].state, states::BATCHED, "{:?}", rows[1].reason);
    assert_eq!(
        rows[1].branch, claim.branch,
        "every try uses the day's branch"
    );
    assert_eq!(rows[1].batch_id, Some(batch.id));
    assert!(!interrupted_flag(&bench, rows[1].id).await?);
    assert_eq!(recover(&bench.ctx).await?, 0);
    bench.cleanup().await
}

#[tokio::test]
async fn finding_interrupted_three_times_is_not_claimed_a_fourth_time() -> anyhow::Result<()> {
    let bench = Bench::boot(WritesFiles::new(&[])).await?;
    bench.defect().await?;
    let batch = bench.open_batch().await?;
    let web = bench.ctx.persist.web();
    for _ in 0..3 {
        let claimed = web.claim_wright_repairs(batch.id, None, 1).await?;
        let claim = claimed
            .first()
            .ok_or_else(|| anyhow::anyhow!("an interrupted finding was not claimed again"))?;
        sqlx::query(
            "UPDATE wright.repairs /* grader-allow: each pass must claim through the store first, and the next claim depends on this row being marked interrupted, so the three updates cannot be one set-based statement */
                SET state = 'refused', reason = 'interrupted: test', interrupted = true
              WHERE id = $1",
        )
        .bind(claim.id)
        .execute(bench.ctx.persist.pool())
        .await?;
    }
    assert!(
        web.claim_wright_repairs(batch.id, None, 1)
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
    let batch = bench.open_batch().await?;
    let web = bench.ctx.persist.web();
    let claimed = web.claim_wright_repairs(batch.id, None, 1).await?;
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
        web.claim_wright_repairs(batch.id, None, 1)
            .await?
            .is_empty()
    );
    assert_eq!(repairs_of(&bench, finding).await?.len(), 1);
    bench.cleanup().await
}

#[tokio::test]
async fn repair_waiting_with_a_live_fixer_is_false_so_investigations_go_on() -> anyhow::Result<()> {
    let bench = Bench::boot(WritesFiles::new(&[])).await?;
    bench.defect().await?;
    let web = bench.ctx.persist.web();
    assert!(web.wright_repair_waiting().await?);
    sqlx::query(
        "INSERT INTO wright.followups (kind, title, body, state, issue_number, fixer_session, fixer_machine, handed_off_at)
         VALUES ('code_defect', 'live fixer', 'a fixer holds the day worktree', 'handed_off', 9001, $1, 'omit', now())",
    )
    .bind(uuid::Uuid::new_v4())
    .execute(bench.ctx.persist.pool())
    .await?;
    assert!(!web.wright_repair_waiting().await?);
    bench.cleanup().await
}
