use serde_json::json;

use super::fixtures_tests::{Bench, FIXED, WritesFiles, sh};
use super::perf::{Perf, PerfEntry, sanitize_label};
use super::policy::is_perf_snapshot;
use super::port::RepairRun;
use super::{states, tick};

const QUERY_FILE: &str = "crates/demo/src/lib.rs";
const QUERY_CODE: &str =
    "pub fn q() { let _ = sqlx::query(\"SELECT id FROM requests WHERE id = 7\"); }\n";
const BEFORE_SQL: &str = ".wright/perf/before.sql";
const AFTER_SQL: &str = ".wright/perf/after.sql";

fn entry(label: &str) -> PerfEntry {
    PerfEntry {
        label: label.to_owned(),
        tier: "read".to_owned(),
        before: Some(BEFORE_SQL.to_owned()),
        after: AFTER_SQL.to_owned(),
        budget_ms: None,
    }
}

fn with_perf() -> RepairRun {
    RepairRun {
        perf: Perf {
            entries: vec![entry("Fetch Request #1")],
            invalid: None,
        },
        ..RepairRun::default()
    }
}

fn sql_files() -> Vec<(&'static str, &'static str)> {
    vec![
        (QUERY_FILE, QUERY_CODE),
        (BEFORE_SQL, "SELECT id FROM requests WHERE id = 7"),
        (AFTER_SQL, "SELECT id FROM requests WHERE id = 7 LIMIT 1"),
    ]
}

#[test]
fn labels_are_reduced_to_lowercase_letters_digits_and_underscores() {
    assert_eq!(sanitize_label("Fetch Request #1"), "fetch_request__1");
    assert_eq!(sanitize_label("  --  "), "query");
    assert_eq!(sanitize_label("ok_name9"), "ok_name9");
}

#[test]
fn perf_list_is_read_from_the_engineer_output_and_a_bad_one_is_named() {
    let good = json!({"perf": [{"label": "a", "tier": "hot", "after": ".wright/perf/a.sql", "budget_ms": 25}]});
    let parsed = Perf::from_output(&good);
    assert_eq!(parsed.entries.len(), 1);
    assert_eq!(parsed.entries[0].budget_ms, Some(25));
    assert_eq!(parsed.entries[0].before, None);
    assert_eq!(Perf::from_output(&json!({"summary": "x"})), Perf::default());
    let bad = Perf::from_output(&json!({"perf": [{"label": "a"}]}));
    assert!(bad.entries.is_empty());
    assert!(bad.invalid.unwrap_or_default().contains("label"));
}

#[test]
fn only_sql_snapshots_under_the_evidence_folder_are_exempt_from_the_path_policy() {
    assert!(is_perf_snapshot(
        "scripts/db-code-graders/perf-evidence/2026-11-18/a_after.sql"
    ));
    assert!(!is_perf_snapshot(
        "scripts/db-code-graders/g6_perf_evidence.sh"
    ));
    assert!(!is_perf_snapshot(
        "scripts/db-code-graders/perf-evidence/x.sh"
    ));
}

fn committed_files(bench: &Bench) -> anyhow::Result<String> {
    sh(
        &bench.batch_dir(),
        &["show", "--name-only", "--format=", "HEAD"],
    )
}

#[tokio::test]
async fn sql_fix_with_perf_entries_commits_the_measurement_block_and_snapshots()
-> anyhow::Result<()> {
    let engineer = WritesFiles::new(&sql_files()).ending(with_perf());
    let bench = Bench::boot(engineer).await?;
    let (_, finding) = bench.defect().await?;
    tick(&bench.ctx, true).await?;
    let repair = bench.repair_of(finding).await?;
    assert_eq!(repair.state, states::BATCHED, "{:?}", repair.reason);
    assert_eq!(
        bench.ctx.runner.perf_runs(),
        vec![format!(
            "scripts/db-perf-check --tier read --label fetch_request__1 --before {BEFORE_SQL} --after {AFTER_SQL}"
        )]
    );
    let message = sh(&bench.batch_dir(), &["log", "-1", "--format=%B"])?;
    assert!(message.contains("Wright-Repair:"), "{message}");
    assert!(
        message.contains("Perf-Label: fetch_request__1"),
        "{message}"
    );
    assert!(message.contains("Perf-Verdict: improved"), "{message}");
    assert!(message.contains("Perf-Query-SHA: abc123"), "{message}");
    assert!(!message.contains("git add"), "{message}");
    let files = committed_files(&bench)?;
    assert!(files.contains("fetch_request__1_after.sql"), "{files}");
    assert!(files.contains("fetch_request__1_before.sql"), "{files}");
    assert!(!files.contains(".wright"), "{files}");
    assert!(!bench.batch_dir().join(".wright/perf").exists());
    bench.cleanup().await
}

#[tokio::test]
async fn measured_regression_goes_back_to_the_engineer_as_a_round_with_the_output()
-> anyhow::Result<()> {
    let engineer = WritesFiles::new(&sql_files())
        .ending(with_perf())
        .then_ending(&sql_files(), with_perf());
    let bench = Bench::boot(engineer).await?;
    if let Ok(mut exits) = bench.ctx.runner.perf_exits.lock() {
        exits.push_back(1);
    }
    let (_, finding) = bench.defect().await?;
    tick(&bench.ctx, true).await?;
    let repair = bench.repair_of(finding).await?;
    assert_eq!(repair.state, states::BATCHED, "{:?}", repair.reason);
    let jobs = bench.ctx.engineer.jobs();
    assert_eq!(jobs.len(), 2);
    assert!(jobs[1].brief.contains("round 2"));
    assert!(
        jobs[0].brief.contains(super::attempt::SQL_BRIEF),
        "a repair brief tells the repairer to compose SQL through views"
    );
    let refusal = jobs[1]
        .inputs
        .iter()
        .find(|(name, _)| name == "refusal.json");
    let value = refusal.map(|(_, v)| v.clone()).unwrap_or_default();
    assert!(
        value["refused"]
            .as_str()
            .unwrap_or_default()
            .contains("regression"),
        "{value}"
    );
    assert!(
        value["output"]
            .as_str()
            .unwrap_or_default()
            .contains("REGRESSION"),
        "{value}"
    );
    assert_eq!(bench.ctx.runner.perf_runs().len(), 2);
    let message = sh(&bench.batch_dir(), &["log", "-1", "--format=%B"])?;
    assert!(message.contains("Perf-Verdict: improved"), "{message}");
    assert!(!message.contains("Perf-Verdict: regression"), "{message}");
    bench.cleanup().await
}

#[tokio::test]
async fn measurement_that_cannot_run_refuses_the_fix_with_the_reason() -> anyhow::Result<()> {
    let engineer = WritesFiles::new(&sql_files()).ending(with_perf());
    let bench = Bench::boot(engineer).await?;
    if let Ok(mut exits) = bench.ctx.runner.perf_exits.lock() {
        exits.extend([2, 2, 2]);
    }
    let (_, finding) = bench.defect().await?;
    let head = bench.scratch.remote_main()?;
    tick(&bench.ctx, true).await?;
    let repair = bench.repair_of(finding).await?;
    assert_eq!(repair.state, states::GATE_FAILED);
    let reason = repair.reason.unwrap_or_default();
    assert!(reason.contains("could not measure"), "{reason}");
    assert_eq!(bench.ctx.runner.perf_runs().len(), 3);
    assert_eq!(sh(&bench.batch_dir(), &["rev-parse", "HEAD"])?, head);
    assert_eq!(sh(&bench.batch_dir(), &["status", "--porcelain"])?, "");
    bench.cleanup().await
}

#[tokio::test]
async fn sql_change_without_perf_entries_is_refused_with_the_instruction() -> anyhow::Result<()> {
    let engineer = WritesFiles::new(&[(QUERY_FILE, QUERY_CODE)]);
    let bench = Bench::boot(engineer).await?;
    let (_, finding) = bench.defect().await?;
    tick(&bench.ctx, true).await?;
    let repair = bench.repair_of(finding).await?;
    assert_eq!(repair.state, states::GATE_FAILED);
    let reason = repair.reason.unwrap_or_default();
    assert!(reason.contains("lists no `perf` entries"), "{reason}");
    assert!(bench.ctx.runner.perf_runs().is_empty());
    let jobs = bench.ctx.engineer.jobs();
    assert_eq!(jobs.len(), 3);
    let refusal = jobs[1]
        .inputs
        .iter()
        .find(|(name, _)| name == "refusal.json");
    let value = refusal.map(|(_, v)| v.clone()).unwrap_or_default();
    assert!(
        value["output"]
            .as_str()
            .unwrap_or_default()
            .contains("query line: "),
        "{value}"
    );
    bench.cleanup().await
}

#[tokio::test]
async fn batch_round_that_changes_sql_is_measured_or_refused() -> anyhow::Result<()> {
    let bench = Bench::boot(WritesFiles::new(&[])).await?;
    bench.open_batch().await?;
    let dir = bench.batch_dir();
    for (path, text) in sql_files() {
        super::scratch_tests::write(&dir.join(path), text)?;
    }
    let runner = &bench.ctx.runner;
    let repo = &bench.ctx.repo;
    let refused =
        super::batch_git::commit_round(runner, repo, &dir, "round", &Perf::default()).await;
    let reason = refused.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(reason.contains("lists no `perf` entries"), "{reason}");
    let perf = with_perf().perf;
    let sha = super::batch_git::commit_round(runner, repo, &dir, "round", &perf).await?;
    assert!(sha.is_some());
    let message = sh(&dir, &["log", "-1", "--format=%B"])?;
    assert!(message.contains("Perf-Verdict: improved"), "{message}");
    assert!(committed_files(&bench)?.contains("fetch_request__1_after.sql"));
    bench.cleanup().await
}

#[tokio::test]
async fn fix_that_touches_no_sql_runs_no_measurement() -> anyhow::Result<()> {
    let bench = Bench::boot(WritesFiles::new(&[("src/calc.sh", FIXED)])).await?;
    let (_, finding) = bench.defect().await?;
    tick(&bench.ctx, true).await?;
    let repair = bench.repair_of(finding).await?;
    assert_eq!(repair.state, states::BATCHED, "{:?}", repair.reason);
    assert!(bench.ctx.runner.perf_runs().is_empty());
    let message = sh(&bench.batch_dir(), &["log", "-1", "--format=%B"])?;
    assert!(!message.contains("Perf-"), "{message}");
    bench.cleanup().await
}
