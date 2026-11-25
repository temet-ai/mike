use crate::store::persistence::web_repo::WebPersist as _;

use super::attempt::AttemptEnd;
use super::batch_land::BatchEnd;
use super::fixtures_tests::{Bench, FIXED, WritesFiles};
use super::{states, tick};

fn fix() -> WritesFiles {
    WritesFiles::new(&[("src/calc.sh", FIXED)])
}

#[tokio::test]
async fn deployed_repair_whose_problem_comes_back_is_ineffective_and_looked_at_again()
-> anyhow::Result<()> {
    let bench = Bench::boot(fix()).await?;
    let (problem, finding) = bench.defect().await?;
    let resource = bench.problem_state(problem).await?.resource;
    tick(&bench.ctx, true, true).await?;
    let batch = bench.only_batch().await?;
    bench.backdate(batch.id).await?;
    let report = tick(&bench.ctx, true, true).await?;
    assert_eq!(report.batches, vec![(batch.id, BatchEnd::Landed)]);
    bench.problems(&[]).await?;
    let live = bench.scratch.move_main("docs/after.md")?;
    bench.ctx.live.set(Some(live));
    tick(&bench.ctx, false, true).await?;
    assert_eq!(bench.repair_of(finding).await?.state, states::DEPLOYED);

    bench.age(problem, 7).await?;
    bench.problems(&[resource.as_str()]).await?;
    let repair = bench.repair_of(finding).await?;
    let report = tick(&bench.ctx, false, true).await?;
    assert_eq!(report.ineffective, vec![repair.id]);
    let ended = bench.repair_of(finding).await?;
    assert_eq!(ended.state, states::INEFFECTIVE);
    assert!(
        ended.reason.unwrap_or_default().contains("came back"),
        "the reason says why"
    );
    bench.cleanup().await
}

#[tokio::test]
async fn finding_whose_problem_healed_before_its_repair_is_still_repaired() -> anyhow::Result<()> {
    let bench = Bench::boot(fix()).await?;
    let (problem, finding) = bench.defect().await?;
    bench.problems(&[]).await?;
    assert_eq!(bench.problem_state(problem).await?.state, "healthy");
    let report = tick(&bench.ctx, true, true).await?;
    assert_eq!(
        report.attempted.map(|(_, end)| end),
        Some(AttemptEnd::Batched)
    );
    assert_eq!(bench.repair_of(finding).await?.state, states::BATCHED);
    bench.cleanup().await
}

#[tokio::test]
async fn fix_dropped_by_a_conflict_is_claimed_again_with_the_earlier_reason() -> anyhow::Result<()>
{
    let bench = Bench::boot(WritesFiles::new(&[("README.md", "a fixed readme\n")])).await?;
    let (_, finding) = bench.defect_in("README.md").await?;
    tick(&bench.ctx, true, true).await?;
    let batch = bench.only_batch().await?;
    bench.scratch.move_main("README.md")?;
    bench.backdate(batch.id).await?;
    tick(&bench.ctx, false, true).await?;
    assert_eq!(bench.repair_of(finding).await?.state, states::DROPPED);

    let today = bench.open_batch().await?;
    let claimed = bench
        .ctx
        .persist
        .web()
        .claim_wright_repairs(today.id, None, 10)
        .await?;
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].finding_id, finding);
    assert!(
        claimed[0]
            .earlier
            .as_deref()
            .is_some_and(|text| text.contains("conflicts with main")),
        "{:?}",
        claimed[0].earlier
    );
    bench.cleanup().await
}
