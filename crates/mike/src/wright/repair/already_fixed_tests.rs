use super::already_fixed::AlreadyFixed;
use super::attempt::AttemptEnd;
use super::fixtures_tests::{Bench, WritesFiles};
use super::port::RepairRun;
use super::{states, tick};

fn says(commit: &str) -> WritesFiles {
    WritesFiles::new(&[]).ending(RepairRun {
        already_fixed: Some(AlreadyFixed {
            commit: commit.to_owned(),
            why: "add uses plus".to_owned(),
        }),
        ..RepairRun::default()
    })
}

#[tokio::test]
async fn repair_already_fixed_by_a_commit_on_main_is_recorded_landed_with_it() -> anyhow::Result<()>
{
    let mut bench = Bench::boot(says("")).await?;
    let sha = bench.scratch.move_main("src/elsewhere.sh")?;
    bench.ctx.engineer = says(&sha);
    let (_, finding) = bench.defect().await?;
    let report = tick(&bench.ctx, true, false).await?;
    assert_eq!(
        report.attempted.map(|(_, end)| end),
        Some(AttemptEnd::Ended)
    );
    let repair = bench.repair_of(finding).await?;
    assert_eq!(repair.state, states::LANDED, "{:?}", repair.reason);
    assert_eq!(repair.merge_sha.as_deref(), Some(sha.as_str()));
    bench.cleanup().await
}

#[tokio::test]
async fn repair_already_fixed_by_a_commit_that_is_not_on_main_is_refused() -> anyhow::Result<()> {
    for commit in ["deadbeefdeadbeef", "--output=x"] {
        let bench = Bench::boot(says(commit)).await?;
        let (_, finding) = bench.defect().await?;
        tick(&bench.ctx, true, true).await?;
        let repair = bench.repair_of(finding).await?;
        assert_eq!(repair.state, states::REFUSED);
        assert!(repair.reason.unwrap_or_default().contains("already fixed"));
        bench.cleanup().await?;
    }
    Ok(())
}
