use crate::store::persistence::web_repo::WebPersist as _;
use std::path::PathBuf;

use super::attempt::AttemptEnd;
use super::fixtures_tests::{Bench, FIXED, WritesFiles, sh};
use super::follow::recover;
use super::{states, tick, worktree};
use crate::store::persistence::wright::RepairUpdate;

fn fix() -> WritesFiles {
    WritesFiles::new(&[("src/calc.sh", FIXED)])
}

async fn state_of(bench: &Bench, finding: i64) -> anyhow::Result<String> {
    Ok(bench.repair_of(finding).await?.state)
}

#[tokio::test]
async fn repair_states_move_in_order_and_the_problem_closes_only_through_its_check()
-> anyhow::Result<()> {
    let bench = Bench::boot(fix()).await?;
    let (problem, finding) = bench.defect().await?;
    let resource = bench.problem_state(problem).await?.resource;
    let base = bench.scratch.remote_main()?;

    tick(&bench.ctx, true).await?;
    let landed = bench.repair_of(finding).await?;
    assert_eq!(landed.state, states::LANDED, "reason: {:?}", landed.reason);
    assert_eq!(
        bench.problem_state(problem).await?.state,
        "open",
        "landing closes no problem"
    );

    bench.problems(&[]).await?;
    assert_eq!(bench.problem_state(problem).await?.state, "healthy");
    tick(&bench.ctx, false).await?;
    assert_eq!(
        state_of(&bench, finding).await?,
        states::LANDED,
        "no live build, no deployed"
    );
    bench.ctx.live.set(Some(base));
    tick(&bench.ctx, false).await?;
    assert_eq!(
        state_of(&bench, finding).await?,
        states::LANDED,
        "a build without the merge is not deployed"
    );

    let live = bench.scratch.move_main("docs/after.md")?;
    bench.ctx.live.set(Some(live));
    let report = tick(&bench.ctx, false).await?;
    assert_eq!(report.deployed, vec![landed.id]);
    assert!(
        report.healthy.is_empty(),
        "a check that passed before the deploy does not count"
    );
    assert_eq!(state_of(&bench, finding).await?, states::DEPLOYED);

    bench.problems(&[resource.as_str()]).await?;
    tick(&bench.ctx, false).await?;
    assert_eq!(
        state_of(&bench, finding).await?,
        states::DEPLOYED,
        "an open problem is not healthy"
    );

    bench.problems(&[]).await?;
    let report = tick(&bench.ctx, false).await?;
    assert_eq!(report.healthy, vec![landed.id]);
    let healthy = bench.repair_of(finding).await?;
    let problem_row = bench.problem_state(problem).await?;
    assert_eq!(healthy.state, states::HEALTHY);
    assert_eq!(healthy.healthy_at, problem_row.healthy_at);
    let stamps = [
        Some(healthy.created_at),
        healthy.prepared_at,
        healthy.gated_at,
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
    let web = bench.ctx.persist.web();

    let claimed = web
        .claim_wright_repairs("/scratch/worktrees", None, 10)
        .await?;
    let found: Vec<i64> = claimed.iter().map(|c| c.finding_id).collect();
    assert_eq!(found, vec![first]);
    assert_eq!(
        claimed[0].worktree,
        format!("/scratch/worktrees/wright-repair-{first}")
    );
    assert_eq!(claimed[0].branch, format!("wright/repair-{first}"));
    assert!(
        web.claim_wright_repairs("/scratch/worktrees", None, 10)
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
    let next = web
        .claim_wright_repairs("/scratch/worktrees", None, 10)
        .await?;
    assert_eq!(
        next.iter().map(|c| c.finding_id).collect::<Vec<_>>(),
        vec![later]
    );
    assert!(
        web.claim_wright_repairs("/scratch/worktrees", None, 10)
            .await?
            .is_empty()
    );
    bench.cleanup().await
}

#[tokio::test]
async fn repair_interrupted_while_editing_is_settled_without_repeating_it() -> anyhow::Result<()> {
    let bench = Bench::boot(fix()).await?;
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
    let repair = bench.repair_of(finding).await?;
    assert_eq!(repair.state, states::REFUSED);
    assert!(repair.reason.unwrap_or_default().starts_with("interrupted"));
    assert!(!path.exists());
    assert!(
        sh(
            &bench.scratch.root,
            &["rev-parse", &format!("refs/heads/{}", claim.branch)]
        )
        .is_ok()
    );
    let again = tick(&bench.ctx, true).await?;
    assert_eq!(
        again.attempted, None,
        "the interrupted finding is not repaired twice"
    );
    assert!(bench.ctx.engineer.jobs().is_empty());
    assert_eq!(bench.scratch.gate_log(), "");
    assert_eq!(recover(&bench.ctx).await?, 0);
    bench.cleanup().await
}

#[tokio::test]
async fn repair_interrupted_after_its_merge_is_recorded_landed_without_a_new_gate()
-> anyhow::Result<()> {
    let bench = Bench::boot(fix()).await?;
    let (_, finding) = bench.defect().await?;
    let report = tick(&bench.ctx, true).await?;
    assert!(matches!(report.attempted, Some((_, AttemptEnd::Landed))));
    let landed = bench.repair_of(finding).await?;
    sqlx::query("UPDATE wright.repairs SET state = 'gating', merge_sha = NULL, landed_at = NULL WHERE id = $1")
        .bind(landed.id)
        .execute(bench.ctx.persist.pool())
        .await?;

    assert_eq!(recover(&bench.ctx).await?, 1);
    let settled = bench.repair_of(finding).await?;
    assert_eq!(settled.state, states::LANDED);
    assert_eq!(settled.merge_sha, landed.merge_sha);
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
