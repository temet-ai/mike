use crate::store::persistence::web_repo::WebPersist as _;

use super::attempt::AttemptEnd;
use super::batch_land::BatchEnd;
use super::fixtures_tests::{Bench, FIXED, WritesFiles, sh};
use super::{batch_states, states, tick};

fn bulk(prefix: &str, count: usize) -> Vec<(String, String)> {
    (0..count)
        .map(|n| (format!("docs/{prefix}{n}.md"), "x\n".to_owned()))
        .collect()
}

fn engineer_with(sets: &[Vec<(String, String)>]) -> WritesFiles {
    let mut engineer = WritesFiles::new(&[]);
    for set in sets {
        let pairs: Vec<(&str, &str)> = set.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
        engineer = engineer.then(&pairs);
    }
    engineer
}

#[tokio::test]
async fn second_fix_of_the_day_reuses_the_worktree_and_is_checked_only_against_its_own_files()
-> anyhow::Result<()> {
    let bench = Bench::boot(engineer_with(&[bulk("a", 15), bulk("b", 10)])).await?;
    let [first, second] = bench.defects(&["docs/a0.md", "docs/b0.md"]).await?[..] else {
        anyhow::bail!("expected two findings");
    };

    tick(&bench.ctx, true).await?;
    let one = bench.repair_of(first).await?;
    assert_eq!(one.state, states::BATCHED, "{:?}", one.reason);
    let dir = bench.batch_dir();
    assert_eq!(
        sh(&dir, &["rev-parse", "HEAD"])?,
        one.commit_sha.clone().unwrap_or_default()
    );

    tick(&bench.ctx, true).await?;
    let two = bench.repair_of(second).await?;
    assert_eq!(two.state, states::BATCHED, "{:?}", two.reason);
    assert_eq!(
        two.base_sha, one.commit_sha,
        "the second fix starts at the first fix's commit"
    );
    assert_eq!(two.worktree, one.worktree);
    assert_eq!(two.batch_id, one.batch_id);
    assert_eq!(
        two.patch["files"].as_array().map(Vec::len),
        Some(10),
        "the fix is judged by its own 10 files, not the day's 25: {}",
        two.patch
    );
    assert_eq!(bench.batches().await?.len(), 1);
    assert_eq!(
        sh(&dir, &["rev-list", "--count", "origin/main..HEAD"])?,
        "2"
    );
    bench.cleanup().await
}

#[tokio::test]
async fn repair_touching_a_crate_runs_one_cargo_check_for_it() -> anyhow::Result<()> {
    let bench = Bench::boot(WritesFiles::new(&[(
        "crates/demo/src/lib.rs",
        "pub fn a() {}\n",
    )]))
    .await?;
    let (_, finding) = bench.defect().await?;
    tick(&bench.ctx, true).await?;
    assert_eq!(bench.repair_of(finding).await?.state, states::BATCHED);
    assert_eq!(
        bench.ctx.runner.cx_runs(),
        vec!["scripts/cx check --lib -p demo".to_owned()]
    );
    bench.cleanup().await
}

#[tokio::test]
async fn batch_closes_at_the_utc_day_boundary_before_any_claim() -> anyhow::Result<()> {
    let bench = Bench::boot(
        WritesFiles::new(&[])
            .then(&[("src/calc.sh", FIXED)])
            .then(&[("src/b.txt", "two\n")]),
    )
    .await?;
    let (_, first) = bench.defect_in("src/calc.sh").await?;
    tick(&bench.ctx, true).await?;
    let yesterday = bench.only_batch().await?;
    bench.backdate(yesterday.id).await?;
    let (_, second) = bench.defect_in("src/b.txt").await?;

    let report = tick(&bench.ctx, true).await?;
    assert_eq!(report.batches, vec![(yesterday.id, BatchEnd::Landed)]);
    assert_eq!(
        report.attempted.map(|(_, end)| end),
        Some(AttemptEnd::Batched)
    );
    let old = bench.repair_of(first).await?;
    let new = bench.repair_of(second).await?;
    assert_eq!(old.state, states::LANDED);
    assert_ne!(
        new.batch_id, old.batch_id,
        "the new day's fix goes into a new batch"
    );
    let batches = bench.batches().await?;
    assert_eq!(batches.len(), 2);
    assert_eq!(batches[0].state, batch_states::LANDED);
    assert_eq!(batches[1].state, batch_states::OPEN);
    assert!(batches[0].landed_at <= Some(new.created_at));
    assert_eq!(
        sh(&bench.batch_dir(), &["show", "HEAD:src/calc.sh"])?,
        FIXED.trim_end(),
        "the new batch starts from main with yesterday's fix"
    );
    bench.cleanup().await
}

#[tokio::test]
async fn empty_batch_at_the_day_boundary_is_removed_without_a_gate() -> anyhow::Result<()> {
    let bench = Bench::boot(WritesFiles::new(&[])).await?;
    let empty = bench.open_batch().await?;
    bench.backdate(empty.id).await?;
    let old_dir = bench
        .ctx
        .repo
        .batch_dir(empty.day.pred_opt().unwrap_or(empty.day));
    assert!(old_dir.exists());

    let report = tick(&bench.ctx, true).await?;
    assert!(report.batches.is_empty());
    assert!(!old_dir.exists(), "the empty batch's worktree is removed");
    let ids: Vec<i64> = bench.batches().await?.iter().map(|b| b.id).collect();
    assert!(!ids.contains(&empty.id));
    assert_eq!(bench.scratch.gate_log(), "");
    assert!(bench.ctx.runner.prs().is_empty());
    bench.cleanup().await
}

#[tokio::test]
async fn batch_waits_for_a_live_fixer_and_no_claim_starts_meanwhile() -> anyhow::Result<()> {
    let bench = Bench::boot(WritesFiles::new(&[("src/calc.sh", FIXED)])).await?;
    let (_, finding) = bench.defect().await?;
    tick(&bench.ctx, true).await?;
    let yesterday = bench.only_batch().await?;
    bench.backdate(yesterday.id).await?;
    let followup = bench.handed_off_followup(41).await?;
    let (_, waiting) = bench.defect_in("src/b.txt").await?;

    let report = tick(&bench.ctx, true).await?;
    assert!(report.batches.is_empty());
    assert_eq!(
        report.attempted, None,
        "no repair starts while a fixer is live"
    );
    assert_eq!(bench.only_batch().await?.state, batch_states::OPEN);
    let claims: i64 =
        sqlx::query_scalar("SELECT count(*) FROM wright.repairs WHERE finding_id = $1")
            .bind(waiting)
            .fetch_one(bench.ctx.persist.pool())
            .await?;
    assert_eq!(claims, 0);

    bench
        .ctx
        .persist
        .web()
        .mark_wright_followup_closed(followup, "dropped", "the issue was closed as not planned")
        .await?;
    let report = tick(&bench.ctx, true).await?;
    assert_eq!(report.batches, vec![(yesterday.id, BatchEnd::Landed)]);
    assert_eq!(bench.repair_of(finding).await?.state, states::LANDED);
    bench.cleanup().await
}

#[tokio::test]
async fn conflicting_fix_is_dropped_at_the_rebase_and_the_rest_lands() -> anyhow::Result<()> {
    let bench = Bench::boot(
        WritesFiles::new(&[])
            .then(&[("src/calc.sh", FIXED)])
            .then(&[("README.md", "a fixed readme\n")]),
    )
    .await?;
    let [kept, conflicted] = bench.defects(&["src/calc.sh", "README.md"]).await?[..] else {
        anyhow::bail!("expected two findings");
    };
    tick(&bench.ctx, true).await?;
    tick(&bench.ctx, true).await?;
    let batch = bench.only_batch().await?;
    bench.scratch.move_main("README.md")?;
    bench.backdate(batch.id).await?;

    let report = tick(&bench.ctx, true).await?;
    assert_eq!(report.batches, vec![(batch.id, BatchEnd::Landed)]);
    let dropped = bench.repair_of(conflicted).await?;
    assert_eq!(dropped.state, states::DROPPED);
    assert!(
        dropped
            .reason
            .unwrap_or_default()
            .contains("conflicts with main"),
        "the reason says why"
    );
    assert_eq!(bench.repair_of(kept).await?.state, states::LANDED);
    assert_eq!(bench.scratch.on_main("README.md")?, "a peer landed this");
    assert_eq!(bench.scratch.on_main("src/calc.sh")?, FIXED.trim_end());
    assert_eq!(bench.scratch.gate_log().lines().count(), 1);
    bench.cleanup().await
}

#[tokio::test]
async fn operator_can_close_a_batch_that_holds_a_fix() -> anyhow::Result<()> {
    let bench = Bench::boot(WritesFiles::new(&[("src/calc.sh", FIXED)])).await?;
    let (_, finding) = bench.defect().await?;
    let empty = bench.open_batch().await?;
    let web = bench.ctx.persist.web();
    assert!(
        web.close_wright_fix_batch_now(empty.id).await?.is_none(),
        "a batch with no fix does not close early"
    );
    tick(&bench.ctx, true).await?;
    let closed = web.close_wright_fix_batch_now(empty.id).await?;
    assert_eq!(
        closed.map(|b| b.state),
        Some(batch_states::CLOSED.to_owned())
    );
    let report = tick(&bench.ctx, true).await?;
    assert_eq!(report.batches, vec![(empty.id, BatchEnd::Landed)]);
    assert_eq!(bench.repair_of(finding).await?.state, states::LANDED);
    bench.cleanup().await
}
