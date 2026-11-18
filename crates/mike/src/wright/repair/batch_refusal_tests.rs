use super::batch_land::BatchEnd;
use super::batch_rounds::BATCH_ROUNDS;
use super::fixtures_tests::{Bench, FIXED, WritesFiles};
use super::landing_tests::three_batched;
use super::{batch_states, states, tick};

fn broken_third() -> WritesFiles {
    WritesFiles::new(&[])
        .then(&[("src/calc.sh", FIXED)])
        .then(&[("src/b.txt", "two\n")])
        .then(&[("src/c.txt", "BROKEN 0\n")])
        .then(&[("src/c.txt", "BROKEN 1\n")])
        .then(&[("src/c.txt", "BROKEN 2\n")])
}

async fn close_the_day(bench: &Bench) -> anyhow::Result<i64> {
    let batch = bench.only_batch().await?;
    bench.backdate(batch.id).await?;
    Ok(batch.id)
}

#[tokio::test]
async fn refused_gate_gets_two_rounds_then_reverts_the_blamed_fix_and_lands_the_rest()
-> anyhow::Result<()> {
    let bench = Bench::boot(broken_third()).await?;
    let [first, second, third] = three_batched(&bench).await?;
    let id = close_the_day(&bench).await?;

    let report = tick(&bench.ctx, true).await?;
    assert_eq!(report.batches, vec![(id, BatchEnd::Landed)]);
    let batch = bench.batches().await?.remove(0);
    assert_eq!(batch.state, batch_states::LANDED, "{:?}", batch.reason);
    assert_eq!(batch.rounds, i32::try_from(BATCH_ROUNDS)?);
    let jobs = bench.ctx.engineer.jobs();
    assert_eq!(jobs.len(), 3 + BATCH_ROUNDS as usize);
    let round = &jobs[3];
    assert!(round.brief.contains("round 1 of 2"));
    let refusal = round.inputs.iter().find(|(name, _)| name == "refusal.json");
    let value = refusal.map(|(_, value)| value.clone()).unwrap_or_default();
    assert!(
        value["output"]
            .as_str()
            .unwrap_or_default()
            .contains("FAIL src/c.txt"),
        "the engineer gets the hook's output: {value}"
    );
    assert_eq!(value["fixes"].as_array().map(Vec::len), Some(3));

    let log = bench.scratch.gate_log();
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(lines.len(), 4, "three refused gates and one passing: {log}");
    assert!(lines[..3].iter().all(|line| line.starts_with("fail ")));
    assert!(lines[3].starts_with("pass "));

    let dropped = bench.repair_of(third).await?;
    assert_eq!(dropped.state, states::DROPPED);
    assert!(dropped.reason.unwrap_or_default().contains("reverted"));
    for kept in [first, second] {
        let repair = bench.repair_of(kept).await?;
        assert_eq!(repair.state, states::LANDED, "{:?}", repair.reason);
        assert_eq!(repair.merge_sha, batch.merge_sha);
    }
    assert_eq!(bench.scratch.on_main("src/calc.sh")?, FIXED.trim_end());
    assert_eq!(bench.scratch.on_main("src/b.txt")?, "two");
    assert!(
        bench.scratch.on_main("src/c.txt").is_err(),
        "the blamed fix is not on main"
    );
    assert_eq!(bench.ctx.runner.prs().len(), 1);
    bench.cleanup().await
}

#[tokio::test]
async fn unattributable_failure_makes_the_batch_refused_and_tells_the_operator()
-> anyhow::Result<()> {
    let engineer = WritesFiles::new(&[])
        .then(&[("src/calc.sh", FIXED)])
        .then(&[("src/b.txt", "two\n")])
        .then(&[("src/c.txt", "SILENT\n")]);
    let bench = Bench::boot(engineer).await?;
    let operator = bench.operator_session().await?;
    let main_before = bench.scratch.remote_main()?;
    let [first, _, third] = three_batched(&bench).await?;
    let id = close_the_day(&bench).await?;

    let report = tick(&bench.ctx, true).await?;
    assert_eq!(report.batches, vec![(id, BatchEnd::Held)]);
    let batch = bench.batches().await?.remove(0);
    assert_eq!(batch.state, batch_states::REFUSED);
    assert!(
        batch
            .reason
            .clone()
            .unwrap_or_default()
            .contains("names no fix's files"),
        "{:?}",
        batch.reason
    );
    assert_eq!(bench.repair_of(first).await?.state, states::BATCHED);
    assert_eq!(bench.repair_of(third).await?.state, states::BATCHED);
    assert_eq!(bench.scratch.remote_main()?, main_before);
    assert!(bench.ctx.runner.prs().is_empty());
    assert!(
        std::path::Path::new(&batch.worktree).exists(),
        "the held branch keeps its worktree"
    );
    let mail = bench.mail_to(operator).await?;
    assert_eq!(mail.len(), 1, "{mail:?}");
    assert!(mail[0].contains("refused") && mail[0].contains(&format!("batch {id}")));

    let again = tick(&bench.ctx, true).await?;
    assert!(
        again.batches.is_empty(),
        "a refused batch is not tried again"
    );
    assert_eq!(bench.mail_to(operator).await?.len(), 1);
    bench.cleanup().await
}

#[tokio::test]
async fn a_round_that_edits_the_gate_is_refused_and_the_blamed_fix_is_reverted()
-> anyhow::Result<()> {
    let engineer = WritesFiles::new(&[])
        .then(&[("src/calc.sh", FIXED)])
        .then(&[("src/b.txt", "two\n")])
        .then(&[("src/c.txt", "BROKEN 0\n")])
        .then(&[("scripts/check.sh", "exit 0\n")])
        .then(&[("scripts/check.sh", "exit 0\n")]);
    let bench = Bench::boot(engineer).await?;
    three_batched(&bench).await?;
    close_the_day(&bench).await?;

    tick(&bench.ctx, true).await?;
    let batch = bench.batches().await?.remove(0);
    assert_eq!(batch.state, batch_states::LANDED, "{:?}", batch.reason);
    assert_eq!(batch.rounds, 0, "a refused round edit does not count");
    assert_ne!(bench.scratch.on_main("scripts/check.sh")?, "exit 0");
    assert!(bench.scratch.on_main("src/c.txt").is_err());
    bench.cleanup().await
}

#[tokio::test]
async fn a_written_gate_log_does_not_make_a_failing_gate_pass() -> anyhow::Result<()> {
    let engineer = WritesFiles::new(&[
        ("gate.log", "pass everything\n"),
        ("out/gate.json", "{\"gate\":\"passed\",\"exit\":0}\n"),
        ("src/calc.sh", "add() { echo $(( $1 - $2 )); }\n# fixed\n"),
    ]);
    let bench = Bench::boot(engineer).await?;
    let (_, finding) = bench.defect().await?;
    tick(&bench.ctx, true).await?;
    let id = close_the_day(&bench).await?;

    let report = tick(&bench.ctx, true).await?;
    assert_eq!(report.batches, vec![(id, BatchEnd::Held)]);
    let batch = bench.batches().await?.remove(0);
    assert_eq!(batch.state, batch_states::FAILED);
    assert_ne!(batch.gate_exit, Some(0));
    let gates = bench.scratch.gate_log();
    assert!(
        gates.lines().all(|line| line.starts_with("fail ")),
        "{gates}"
    );
    assert_eq!(bench.repair_of(finding).await?.state, states::DROPPED);
    assert!(bench.ctx.runner.prs().is_empty());
    assert_eq!(bench.scratch.remote_branch(&batch.branch), None);
    assert!(
        bench.scratch.on_main("src/calc.sh")?.contains("- $2"),
        "main keeps its defect"
    );
    bench.cleanup().await
}

#[tokio::test]
async fn shutdown_while_gating_leaves_the_batch_gating_and_a_restart_lands_it() -> anyhow::Result<()>
{
    let engineer = WritesFiles::new(&[])
        .then(&[("src/calc.sh", FIXED)])
        .then(&[("src/c.txt", "BROKEN 0\n")]);
    let bench = Bench::boot(engineer).await?;
    bench.defects(&["src/calc.sh", "src/c.txt"]).await?;
    tick(&bench.ctx, true).await?;
    tick(&bench.ctx, true).await?;
    let id = close_the_day(&bench).await?;
    bench.ctx.shutdown.cancel();

    let report = tick(&bench.ctx, true).await?;
    assert_eq!(report.batches, vec![(id, BatchEnd::Interrupted)]);
    let batch = bench.batches().await?.remove(0);
    assert_eq!(batch.state, batch_states::GATING, "{:?}", batch.reason);
    assert_eq!(bench.scratch.gate_log().lines().count(), 1);
    assert_eq!(
        bench.ctx.engineer.jobs().len(),
        2,
        "no round starts while mike stops"
    );

    let restarted = bench.restarted(WritesFiles::new(&[]).then(&[("src/c.txt", "fixed\n")]));
    let report = tick(&restarted, true).await?;
    assert_eq!(report.batches, vec![(id, BatchEnd::Landed)]);
    let landed = bench.batches().await?.remove(0);
    assert_eq!(landed.state, batch_states::LANDED, "{:?}", landed.reason);
    assert_eq!(landed.rounds, 1);
    assert_eq!(bench.scratch.on_main("src/c.txt")?, "fixed");
    let log = bench.scratch.gate_log();
    assert_eq!(log.lines().count(), 3, "{log}");
    bench.cleanup().await
}
