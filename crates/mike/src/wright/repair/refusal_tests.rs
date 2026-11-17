use super::attempt::AttemptEnd;
use super::feedback::{FEEDBACK_INPUT, MAX_ROUNDS};
use super::fixtures_tests::{BUGGY, Bench, FIXED, WritesFiles, sh};
use super::port::RepairRun;
use super::{states, tick};
use crate::store::persistence::wright::RepairRow;

async fn ended(engineer: WritesFiles) -> anyhow::Result<(Bench, i64, RepairRow)> {
    let bench = Bench::boot(engineer).await?;
    let (_, finding) = bench.defect().await?;
    let main_before = bench.scratch.remote_main()?;
    tick(&bench.ctx, true).await?;
    let repair = bench.repair_of(finding).await?;
    assert_eq!(
        bench.scratch.remote_main()?,
        main_before,
        "nothing lands on main"
    );
    assert!(
        !bench.worktree(finding).exists(),
        "an ended attempt's worktree is removed"
    );
    assert!(repair.reason.is_some(), "an ended attempt keeps its reason");
    Ok((bench, finding, repair))
}

fn local_branch(bench: &Bench, branch: &str) -> anyhow::Result<String> {
    sh(
        &bench.scratch.root,
        &["rev-parse", &format!("refs/heads/{branch}")],
    )
}

#[tokio::test]
async fn repair_that_edits_the_test_runner_is_refused_before_any_gate() -> anyhow::Result<()> {
    let engineer = WritesFiles::new(&[("scripts/check.sh", "exit 0\n"), ("src/calc.sh", BUGGY)]);
    let (bench, _, repair) = ended(engineer).await?;
    assert_eq!(repair.state, states::REFUSED);
    let reason = repair.reason.clone().unwrap_or_default();
    assert!(
        reason.contains("scripts/check.sh") && reason.contains("gate"),
        "{reason}"
    );
    assert_eq!(
        bench.scratch.gate_log(),
        "",
        "a refused patch never reaches the push hook"
    );
    assert_eq!(
        bench.scratch.remote_branch(&repair.branch),
        None,
        "a refused patch is never pushed"
    );
    let kept = local_branch(&bench, &repair.branch)?;
    assert_eq!(
        Some(kept.clone()),
        repair.commit_sha,
        "the refused patch stays on its branch"
    );
    assert_eq!(
        sh(
            &bench.scratch.root,
            &["show", &format!("{kept}:scripts/check.sh")]
        )?,
        "exit 0"
    );
    bench.cleanup().await
}

#[tokio::test]
async fn repair_with_a_written_gate_passed_log_still_fails_the_gate() -> anyhow::Result<()> {
    let engineer = WritesFiles::new(&[
        ("gate.log", "pass everything\n"),
        ("out/gate.json", "{\"gate\":\"passed\",\"exit\":0}\n"),
        ("src/calc.sh", "add() { echo $(( $1 - $2 )); }\n# fixed\n"),
    ]);
    let (bench, _, repair) = ended(engineer).await?;
    assert_eq!(repair.state, states::GATE_FAILED);
    assert_ne!(repair.gate_exit, Some(0));
    let commit = repair.commit_sha.clone().unwrap_or_default();
    let gates = bench.scratch.gate_log();
    assert_eq!(gates.lines().count(), MAX_ROUNDS as usize, "{gates}");
    assert!(
        gates.lines().all(|line| line.starts_with("fail ")),
        "{gates}"
    );
    assert!(gates.ends_with(&format!("fail {commit}\n")), "{gates}");
    let jobs = bench.ctx.engineer.jobs();
    assert_eq!(jobs.len(), MAX_ROUNDS as usize);
    assert!(
        jobs[0]
            .inputs
            .iter()
            .all(|(name, _)| name != FEEDBACK_INPUT)
    );
    for later in &jobs[1..] {
        let refusal = later.inputs.iter().find(|(name, _)| name == FEEDBACK_INPUT);
        assert!(
            refusal.is_some_and(|(_, value)| value["refused"]
                .as_str()
                .is_some_and(|r| r.contains("push hook did not pass"))),
            "a later round carries the gate's refusal"
        );
    }
    assert!(
        repair
            .reason
            .clone()
            .unwrap_or_default()
            .contains("push hook did not pass")
    );
    assert_eq!(bench.scratch.remote_branch(&repair.branch), None);
    assert_eq!(local_branch(&bench, &repair.branch)?, commit);
    assert!(
        bench.ctx.runner.prs().is_empty(),
        "no pull request for a failed gate"
    );
    bench.cleanup().await
}

#[tokio::test]
async fn repair_whose_migration_flips_a_toggle_is_refused() -> anyhow::Result<()> {
    let engineer = WritesFiles::new(&[
        ("src/calc.sh", FIXED),
        (
            "crates/mike-base/src/store/persistence/migrations/20991231000000_tidy.sql",
            "UPDATE config.feature_toggles SET enabled = true WHERE key = 'w' || 'right';\n",
        ),
    ]);
    let (bench, _, repair) = ended(engineer).await?;
    assert_eq!(repair.state, states::REFUSED);
    assert!(repair.reason.unwrap_or_default().contains("feature toggle"));
    assert_eq!(bench.scratch.gate_log(), "");
    bench.cleanup().await
}

#[tokio::test]
async fn repair_with_no_change_is_refused() -> anyhow::Result<()> {
    let (bench, _, repair) = ended(WritesFiles::new(&[])).await?;
    assert_eq!(repair.state, states::REFUSED);
    assert!(
        repair
            .reason
            .unwrap_or_default()
            .contains("changed no files")
    );
    bench.cleanup().await
}

async fn failed_repair_runs(bench: &Bench, problem: i64, runs: i64) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO wright.investigations (mode, problem_id, session_uuid, state, host,
                                            deadline_at, ended_at, end_reason)
         SELECT 'repair', $1, gen_random_uuid(), 'failed', 'test', now(), now(), 'no repair.json'
           FROM generate_series(1, $2)",
    )
    .bind(problem)
    .bind(runs)
    .execute(bench.ctx.persist.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn repair_whose_engineer_run_fails_again_is_refused_with_the_reason() -> anyhow::Result<()> {
    let bench = Bench::boot(fix_ending(RepairRun {
        failure: Some("killed at the wall-time limit".to_owned()),
        ..RepairRun::default()
    }))
    .await?;
    let (problem, finding) = bench.defect().await?;
    failed_repair_runs(&bench, problem, 2).await?;
    tick(&bench.ctx, true).await?;
    let repair = bench.repair_of(finding).await?;
    assert!(!bench.worktree(finding).exists());
    assert_eq!(repair.state, states::REFUSED);
    assert!(
        repair
            .reason
            .unwrap_or_default()
            .contains("wall-time limit")
    );
    assert_eq!(bench.scratch.gate_log(), "");
    bench.cleanup().await
}

fn fix_ending(result: RepairRun) -> WritesFiles {
    WritesFiles::new(&[("src/calc.sh", FIXED)]).ending(result)
}

#[tokio::test]
async fn repair_whose_first_engineer_run_fails_is_released_and_tried_again() -> anyhow::Result<()> {
    let bench = Bench::boot(fix_ending(RepairRun {
        failure: Some("left no repair.json".to_owned()),
        ..RepairRun::default()
    }))
    .await?;
    let (problem, finding) = bench.defect().await?;
    failed_repair_runs(&bench, problem, 1).await?;
    let report = tick(&bench.ctx, true).await?;
    assert!(matches!(
        report.attempted,
        Some((_, AttemptEnd::NotStarted))
    ));
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM wright.repairs WHERE finding_id = $1")
        .bind(finding)
        .fetch_one(bench.ctx.persist.pool())
        .await?;
    assert_eq!(left, 0, "a first failed run releases the finding");
    assert!(!bench.worktree(finding).exists());
    assert_eq!(bench.scratch.gate_log(), "");
    bench.cleanup().await
}

#[tokio::test]
async fn repair_whose_engineer_run_cannot_start_leaves_no_trace_and_retries() -> anyhow::Result<()>
{
    let bench = Bench::boot(fix_ending(RepairRun {
        not_started: Some("another run is live".to_owned()),
        ..RepairRun::default()
    }))
    .await?;
    let (_, finding) = bench.defect().await?;
    let report = tick(&bench.ctx, true).await?;
    assert!(matches!(
        report.attempted,
        Some((_, AttemptEnd::NotStarted))
    ));
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM wright.repairs WHERE finding_id = $1")
        .bind(finding)
        .fetch_one(bench.ctx.persist.pool())
        .await?;
    assert_eq!(left, 0, "a run that never started is not an attempt");
    assert!(!bench.worktree(finding).exists());
    assert!(local_branch(&bench, &format!("wright/repair-{finding}")).is_err());
    let again = tick(&bench.ctx, true).await?;
    assert!(
        matches!(again.attempted, Some((_, AttemptEnd::NotStarted))),
        "the finding is claimed again later"
    );
    bench.cleanup().await
}
