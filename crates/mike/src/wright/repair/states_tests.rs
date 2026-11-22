use crate::store::persistence::web_repo::WebPersist as _;

use super::attempt::AttemptEnd;
use super::batch_land::BatchEnd;
use super::fixtures_tests::{Bench, FIXED, WritesFiles};
use super::{batch_states, states, tick};
use crate::store::persistence::wright::RepairUpdate;

fn fix() -> WritesFiles {
    WritesFiles::new(&[("src/calc.sh", FIXED)])
}

async fn state_of(bench: &Bench, finding: i64) -> anyhow::Result<String> {
    Ok(bench.repair_of(finding).await?.state)
}

async fn land_the_day(bench: &Bench) -> anyhow::Result<()> {
    let batch = bench.only_batch().await?;
    bench.backdate(batch.id).await?;
    let report = tick(&bench.ctx, true, true).await?;
    assert_eq!(report.batches, vec![(batch.id, BatchEnd::Landed)]);
    Ok(())
}

#[tokio::test]
async fn repair_states_move_in_order_and_the_problem_closes_only_through_its_check()
-> anyhow::Result<()> {
    let bench = Bench::boot(fix()).await?;
    let (problem, finding) = bench.defect().await?;
    let resource = bench.problem_state(problem).await?.resource;
    let base = bench.scratch.remote_main()?;

    let report = tick(&bench.ctx, true, true).await?;
    assert_eq!(
        report.attempted.map(|(_, end)| end),
        Some(AttemptEnd::Batched)
    );
    assert_eq!(state_of(&bench, finding).await?, states::BATCHED);
    land_the_day(&bench).await?;
    let landed = bench.repair_of(finding).await?;
    assert_eq!(landed.state, states::LANDED, "reason: {:?}", landed.reason);
    assert_eq!(
        bench.problem_state(problem).await?.state,
        "open",
        "landing closes no problem"
    );

    bench.problems(&[]).await?;
    assert_eq!(bench.problem_state(problem).await?.state, "healthy");
    tick(&bench.ctx, false, true).await?;
    assert_eq!(
        state_of(&bench, finding).await?,
        states::LANDED,
        "no live build, no deployed"
    );
    bench.ctx.live.set(Some(base));
    tick(&bench.ctx, false, true).await?;
    assert_eq!(
        state_of(&bench, finding).await?,
        states::LANDED,
        "a build without the merge is not deployed"
    );

    let live = bench.scratch.move_main("docs/after.md")?;
    bench.ctx.live.set(Some(live));
    let report = tick(&bench.ctx, false, true).await?;
    assert_eq!(report.deployed, vec![landed.id]);
    assert!(
        report.healthy.is_empty(),
        "a check that passed before the deploy does not count"
    );
    assert_eq!(state_of(&bench, finding).await?, states::DEPLOYED);

    bench.problems(&[resource.as_str()]).await?;
    tick(&bench.ctx, false, true).await?;
    assert_eq!(
        state_of(&bench, finding).await?,
        states::DEPLOYED,
        "an open problem is not healthy"
    );

    bench.problems(&[]).await?;
    let report = tick(&bench.ctx, false, true).await?;
    assert_eq!(report.healthy, vec![landed.id]);
    let healthy = bench.repair_of(finding).await?;
    let problem_row = bench.problem_state(problem).await?;
    assert_eq!(healthy.state, states::HEALTHY);
    assert_eq!(healthy.healthy_at, problem_row.healthy_at);
    let stamps = [
        Some(healthy.created_at),
        healthy.prepared_at,
        healthy.landed_at,
        healthy.deployed_at,
        healthy.healthy_at,
    ];
    assert!(stamps.iter().all(Option::is_some), "{stamps:?}");
    assert!(
        stamps.windows(2).all(|w| w[0] <= w[1]),
        "states out of order: {stamps:?}"
    );
    bench.cleanup().await
}

#[tokio::test]
async fn two_machines_claiming_at_once_get_one_repair_per_problem() -> anyhow::Result<()> {
    let bench = Bench::boot(fix()).await?;
    let ids = bench.problems(&["a", "b"]).await?;
    let [a, b] = ids.as_slice() else {
        anyhow::bail!("expected two problems, got {ids:?}");
    };
    for problem in [*a, *a, *b] {
        bench
            .finding(problem, "code_defect", true, Some("crates/mike/src/x.rs"))
            .await?;
    }
    let batch = bench.open_batch().await?;
    let web = bench.ctx.persist.web();
    let (one, two) = tokio::join!(
        web.claim_wright_repairs(batch.id, Some(uuid::Uuid::new_v4()), 10),
        web.claim_wright_repairs(batch.id, Some(uuid::Uuid::new_v4()), 10),
    );
    let mut problems: Vec<i64> = one?
        .iter()
        .chain(two?.iter())
        .map(|c| c.problem_id)
        .collect();
    problems.sort_unstable();
    assert_eq!(
        problems,
        vec![*a, *b],
        "one live repair per problem across machines"
    );
    bench.cleanup().await
}

#[tokio::test]
async fn repair_picker_gives_one_repair_to_each_established_code_defect() -> anyhow::Result<()> {
    let bench = Bench::boot(fix()).await?;
    let ids = bench.problems(&["a", "b", "c"]).await?;
    let [a, b, c] = ids.as_slice() else {
        anyhow::bail!("expected three problems, got {ids:?}");
    };
    let first = bench
        .finding(*a, "code_defect", true, Some("crates/mike/src/x.rs"))
        .await?;
    let later = bench
        .finding(*a, "code_defect", true, Some("crates/mike/src/x.rs"))
        .await?;
    bench
        .finding(*b, "code_defect", false, Some("crates/mike/src/y.rs"))
        .await?;
    bench
        .finding(*c, "config", true, Some("crates/mike/src/z.rs"))
        .await?;
    let batch = bench.open_batch().await?;
    let web = bench.ctx.persist.web();

    let claimed = web.claim_wright_repairs(batch.id, None, 10).await?;
    let found: Vec<i64> = claimed.iter().map(|c| c.finding_id).collect();
    assert_eq!(found, vec![first]);
    assert_eq!(claimed[0].worktree, batch.worktree);
    assert_eq!(claimed[0].branch, batch.branch);
    assert_eq!(claimed[0].batch_id, batch.id);
    assert!(
        web.claim_wright_repairs(batch.id, None, 10)
            .await?
            .is_empty()
    );

    web.advance_wright_repair(&RepairUpdate {
        id: claimed[0].id,
        to: states::REFUSED,
        from: &[states::EDITING],
        reason: Some("test".to_owned()),
        ..RepairUpdate::default()
    })
    .await?;
    let next = web.claim_wright_repairs(batch.id, None, 10).await?;
    assert_eq!(
        next.iter().map(|c| c.finding_id).collect::<Vec<_>>(),
        vec![later]
    );
    assert!(
        web.claim_wright_repairs(batch.id, None, 10)
            .await?
            .is_empty()
    );
    bench.cleanup().await
}

#[tokio::test]
async fn a_closed_batch_takes_no_claim() -> anyhow::Result<()> {
    let bench = Bench::boot(fix()).await?;
    bench.defect().await?;
    let batch = bench.open_batch().await?;
    sqlx::query("UPDATE wright.fix_batches SET state = 'closed', closed_at = now() WHERE id = $1")
        .bind(batch.id)
        .execute(bench.ctx.persist.pool())
        .await?;
    assert!(
        bench
            .ctx
            .persist
            .web()
            .claim_wright_repairs(batch.id, None, 10)
            .await?
            .is_empty()
    );
    bench.cleanup().await
}

#[tokio::test]
async fn batch_interrupted_after_its_merge_is_recorded_landed_without_a_new_gate()
-> anyhow::Result<()> {
    let bench = Bench::boot(fix()).await?;
    let (_, finding) = bench.defect().await?;
    tick(&bench.ctx, true, true).await?;
    land_the_day(&bench).await?;
    let landed = bench.repair_of(finding).await?;
    let batch = bench.batches().await?.remove(0);
    sqlx::query("UPDATE wright.fix_batches SET state = 'gating', merge_sha = NULL, landed_at = NULL WHERE id = $1")
        .bind(batch.id)
        .execute(bench.ctx.persist.pool())
        .await?;
    sqlx::query("UPDATE wright.repairs SET state = 'batched', merge_sha = NULL, landed_at = NULL WHERE id = $1")
        .bind(landed.id)
        .execute(bench.ctx.persist.pool())
        .await?;

    let report = tick(&bench.ctx, true, true).await?;
    assert_eq!(report.batches, vec![(batch.id, BatchEnd::Landed)]);
    let settled = bench.repair_of(finding).await?;
    assert_eq!(settled.state, states::LANDED);
    assert_eq!(settled.merge_sha, landed.merge_sha);
    let row = bench.batches().await?.remove(0);
    assert_eq!(row.state, batch_states::LANDED);
    assert_eq!(
        bench.scratch.gate_log().lines().count(),
        1,
        "recovery never gates again"
    );
    assert_eq!(
        bench.ctx.runner.prs().len(),
        1,
        "recovery never opens another pull request"
    );
    bench.cleanup().await
}

#[tokio::test]
async fn repair_that_lands_posts_a_landed_event_to_its_checks_inbox() -> anyhow::Result<()> {
    let bench = Bench::boot(fix()).await?;
    let (_, finding) = bench.defect().await?;
    tick(&bench.ctx, true, true).await?;
    land_the_day(&bench).await?;
    let repair = bench.repair_of(finding).await?;
    assert_eq!(repair.state, states::LANDED, "reason: {:?}", repair.reason);
    let inbox = bench
        .ctx
        .persist
        .web()
        .unread_wright_inbox(super::scratch_tests::CHECK_NAME)
        .await?;
    let landed = inbox
        .iter()
        .find(|event| event.kind == "repair" && event.ref_id == repair.id);
    assert!(
        landed.is_some_and(|event| event.state == states::LANDED
            && event.summary.contains(&format!("repair {}", repair.id))),
        "{inbox:?}"
    );
    bench.cleanup().await
}
