use std::path::Path;

use super::definition::{FleetDefinition, GateDefinition, TaskDefinition};
use super::gate_blame::Evidence;
use super::gate_queue::{
    Blame, Decision, RETRY_AFTER, Suspect, backing_off, blame, commit_to_check, decide, one_line,
    pre_existing_mail, seat_mail, unchecked_from,
};

fn ended(
    state: &str,
    exit_code: Option<i32>,
    finished: chrono::DateTime<chrono::Utc>,
) -> crate::store::persistence::fleet::TreeCheckRow {
    crate::store::persistence::fleet::TreeCheckRow {
        id: 936,
        run_id: "run#gate".into(),
        state: state.into(),
        state_after: Some("unverified-source".into()),
        command: "scripts/check".into(),
        started_at: finished,
        finished_at: Some(finished),
        exit_code,
        output: String::new(),
        askers: Vec::new(),
        told: Vec::new(),
    }
}

#[test]
fn a_gate_that_ended_without_a_result_is_not_retried_at_once() {
    let now = chrono::Utc::now();
    let failed = ended("61eced52", None, now - chrono::Duration::seconds(15));
    assert!(backing_off(Some(&failed), "61eced52", now).is_some());
    assert!(backing_off(Some(&failed), "abcdef01", now).is_none());
    let old = ended(
        "61eced52",
        None,
        now - RETRY_AFTER - chrono::Duration::seconds(1),
    );
    assert!(backing_off(Some(&old), "61eced52", now).is_none());
    let finished = ended("61eced52", Some(1), now);
    assert!(backing_off(Some(&finished), "61eced52", now).is_none());
    assert!(backing_off(None, "61eced52", now).is_none());
}
use super::gate_rerun::{
    Rerun, cleared, configured, named, only_timeouts_that_pass, rerun_command, rerun_lines,
};
use super::managed_test_support as managed;

async fn git(dir: &Path, args: &[&str]) -> anyhow::Result<String> {
    let output = tokio::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .await?;
    anyhow::ensure!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

async fn init(dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    git(dir, &["init", "-q", "-b", "main"]).await?;
    git(dir, &["config", "user.email", "t@example.com"]).await?;
    git(dir, &["config", "user.name", "t"]).await?;
    Ok(())
}

async fn commit(dir: &Path, file: &str, seat: &str) -> anyhow::Result<String> {
    std::fs::write(dir.join(file), format!("{file}\n"))?;
    git(dir, &["add", "-A"]).await?;
    let message =
        format!("add {file}\n\nFleet-Seat: {seat} (6c8c2d1e-0000-4000-8000-000000000001)\n");
    git(dir, &["commit", "-q", "-m", &message]).await?;
    git(dir, &["rev-parse", "HEAD"]).await
}

fn definition(repo: &Path) -> FleetDefinition {
    let mut def = FleetDefinition {
        repo: repo.to_string_lossy().into_owned(),
        branch: "main".to_string(),
        gate: GateDefinition {
            command: "test ! -e bad.txt || { echo 'bad.txt: broken'; exit 1; }".to_string(),
            ..GateDefinition::default()
        },
        ..FleetDefinition::default()
    };
    for seat in ["seat-a", "seat-b", "seat-c"] {
        def.tasks.insert(
            seat.to_string(),
            TaskDefinition {
                seat: Some(seat.to_string()),
                session: Some("6c8c2d1e-0000-4000-8000-000000000001".to_string()),
                ..TaskDefinition::default()
            },
        );
    }
    def
}

#[test]
fn a_pushed_commit_with_no_verdict_starts_a_run() {
    assert_eq!(decide(false, None), Decision::Start);
    assert_eq!(decide(true, None), Decision::Checked);
}

#[test]
fn pushes_during_a_run_wait_for_it_and_become_one_run_at_the_newest_commit() {
    let (first, tip_after_two_pushes) = ("a1", "c3");
    assert_eq!(decide(false, None), Decision::Start, "{first} starts");
    for _ in 0..3 {
        assert_eq!(
            decide(false, Some(first)),
            Decision::Running,
            "a push while {first} runs queues nothing"
        );
    }
    assert_eq!(
        decide(false, None),
        Decision::Start,
        "when the run ends one run starts at {tip_after_two_pushes}, not one per push"
    );
    assert_eq!(decide(true, None), Decision::Checked);
}

#[test]
fn the_pre_existing_mail_names_the_commit_the_failing_lines_and_the_log() {
    let body = pre_existing_mail(
        "scripts/gate check",
        "46d7b05f1234567",
        &[],
        "ok\nFAIL (4810/6427) mike::cli x\n",
        Path::new("/slot/gate.log"),
    );
    assert!(body.contains("PRE-EXISTING"), "{body}");
    assert!(body.contains("46d7b05f12"), "{body}");
    assert!(body.contains("  FAIL mike::cli x"), "{body}");
    assert!(body.contains("/slot/gate.log"), "{body}");
}

#[tokio::test]
async fn a_failure_at_a_head_with_no_seat_commit_has_no_blame_so_it_is_pre_existing()
-> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let repo = root.path().join("project");
    init(&repo).await?;
    let start = commit(&repo, "bad.txt", "someone-before-the-run").await?;
    let def = definition(&repo);
    assert_eq!(blame(&def, None, &start, "bad.txt: broken").await, None);
    Ok(())
}

#[test]
fn a_branch_ahead_of_the_last_passing_commit_is_unchecked() {
    assert_eq!(unchecked_from("c3", None), Some("c3".to_string()));
    assert_eq!(unchecked_from("c3", Some(false)), Some("c3".to_string()));
    assert_eq!(unchecked_from("c3", Some(true)), None);
}

#[tokio::test]
async fn the_commit_to_check_is_the_pushed_one_when_the_branch_has_a_remote() -> anyhow::Result<()>
{
    let root = tempfile::tempdir()?;
    let remote = root.path().join("remote.git");
    std::fs::create_dir_all(&remote)?;
    git(&remote, &["init", "-q", "--bare", "-b", "main"]).await?;
    let repo = root.path().join("project");
    init(&repo).await?;
    git(
        &repo,
        &["remote", "add", "origin", &remote.to_string_lossy()],
    )
    .await?;
    let pushed = commit(&repo, "a.txt", "seat-a").await?;
    git(&repo, &["push", "-q", "-u", "origin", "main"]).await?;
    commit(&repo, "b.txt", "seat-a").await?;
    let def = definition(&repo);
    let target = commit_to_check(&def)
        .await
        .ok_or_else(|| anyhow::anyhow!("no target"))?;
    assert_eq!((target.commit, target.pushed), (pushed, true));
    assert_eq!(target.slot, repo.join(".claude/worktrees/mike-gate"));
    Ok(())
}

#[tokio::test]
async fn a_failing_run_names_the_first_commit_that_failed_and_its_seat() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let repo = root.path().join("project");
    init(&repo).await?;
    let good = commit(&repo, "ok.txt", "seat-a").await?;
    let breaker = commit(&repo, "bad.txt", "seat-b").await?;
    commit(&repo, "later.txt", "seat-c").await?;
    let tip = commit(&repo, "last.txt", "seat-a").await?;
    let def = definition(&repo);
    git(&repo, &["checkout", "-q", "--detach", &tip]).await?;
    let (code, output) = managed::gate_command(&repo, &def.gate.command, "fleet-x").await?;
    assert_eq!((code, output.as_str()), (Some(1), "bad.txt: broken\n"));
    let found = blame(&def, Some(&good), &tip, &output).await;
    assert_eq!(
        found,
        Some(Blame::Suspects(vec![Suspect {
            commit: breaker,
            seat: "seat-b".to_string(),
            since: 3,
            evidence: Evidence::Touches("bad.txt".to_string()),
        }]))
    );
    Ok(())
}

#[tokio::test]
async fn when_no_commit_touches_a_named_file_no_seat_is_blamed() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let repo = root.path().join("project");
    init(&repo).await?;
    let good = commit(&repo, "ok.txt", "seat-a").await?;
    commit(&repo, "x.txt", "seat-b").await?;
    let tip = commit(&repo, "y.txt", "seat-c").await?;
    let def = definition(&repo);
    let found = blame(&def, Some(&good), &tip, "boom").await;
    assert_eq!(found, Some(Blame::NoMatch { since: 2 }));
    Ok(())
}

#[tokio::test]
async fn a_missing_module_error_blames_the_commit_that_declares_it_not_the_first_commit()
-> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let repo = root.path().join("project");
    init(&repo).await?;
    let good = commit(&repo, "ok.txt", "seat-a").await?;
    commit(&repo, "docs.md", "seat-c").await?;
    let declares = commit(&repo, "changes_mod.rs", "seat-b").await?;
    let tip = commit(&repo, "later.md", "seat-c").await?;
    let def = definition(&repo);
    let named = "error[E0583]: file not found for module `mine`\n --> changes_mod.rs:1:1\n";
    assert_eq!(
        blame(&def, Some(&good), &tip, named).await,
        Some(Blame::Suspects(vec![Suspect {
            commit: declares,
            seat: "seat-b".to_string(),
            since: 3,
            evidence: Evidence::Located("changes_mod.rs".to_string()),
        }]))
    );
    let unnamed =
        "error[E0583]: file not found for module `mine`\n = help: create file \"mine.rs\"\n";
    assert_eq!(
        blame(&def, Some(&good), &tip, unnamed).await,
        Some(Blame::NoMatch { since: 3 })
    );
    Ok(())
}

#[tokio::test]
async fn a_commit_not_made_by_a_seat_in_this_run_is_never_blamed() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let repo = root.path().join("project");
    init(&repo).await?;
    let baseline = commit(&repo, "bad.txt", "seat-before-this-run").await?;
    let def = definition(&repo);
    assert_eq!(blame(&def, None, &baseline, "bad.txt: broken").await, None);
    Ok(())
}

#[test]
fn the_mail_to_the_seat_carries_the_failing_lines_and_the_commit() {
    let suspect = Suspect {
        commit: "0123456789abcdef".to_string(),
        seat: "seat-b".to_string(),
        since: 3,
        evidence: Evidence::Touches("bad.txt".to_string()),
    };
    let blamed = Blame::Suspects(vec![suspect.clone()]);
    let output = "compiling\nbad.txt: broken\nerror: gate failed";
    let mail = seat_mail(
        "make check",
        "fedcba9876543210",
        &suspect,
        &[],
        output,
        Path::new("/srv/p-mike-gate/.mike-gate-output.log"),
    );
    assert!(mail.contains("failed at commit fedcba9876"), "{mail}");
    assert!(
        mail.contains("Your commit 0123456789 touches `bad.txt`"),
        "{mail}"
    );
    assert!(mail.contains("Failing-test inventory unparsed"), "{mail}");
    let line = one_line("make check", "fedcba9876543210", &blamed, &[]);
    assert_eq!(
        line,
        "[mike gate] `make check` failed at commit fedcba9876; first suspect: 0123456789 by seat-b"
    );
}

#[test]
fn with_no_match_the_line_for_the_control_seats_names_no_seat() {
    let line = one_line(
        "make check",
        "fedcba9876543210",
        &Blame::NoMatch { since: 4 },
        &[],
    );
    assert_eq!(
        line,
        "[mike gate] `make check` failed at commit fedcba9876; none of the 4 commit(s) since the last pass adds a name or touches a file the failing output names, so no seat is named and the verdict is yours"
    );
}

#[test]
fn the_timeout_pattern_names_each_timed_out_test_once() {
    let output = "test a::slow ... TIMEOUT\ntest b::ok ... ok\ntest a::slow ... TIMEOUT\ntest c::it's ... TIMEOUT";
    let pattern = r"test (?P<test>\S+) \.\.\. TIMEOUT";
    assert_eq!(
        named(pattern, output),
        vec!["a::slow".to_string(), "c::it's".to_string()]
    );
    assert!(named("(", output).is_empty());
    assert!(named(pattern, "all fine").is_empty());
}

#[test]
fn a_rerun_runs_the_one_named_test_with_the_name_quoted() {
    assert_eq!(
        rerun_command("t --only {test}", "a::slow"),
        "t --only 'a::slow'"
    );
    assert_eq!(rerun_command("t {test}", "it's"), "t 'it'\\''s'");
}

#[test]
fn without_both_fields_nothing_is_rerun() {
    let both = GateDefinition {
        command: "g".to_string(),
        timeout_pattern: "x".to_string(),
        rerun_command: "t {test}".to_string(),
        ..GateDefinition::default()
    };
    assert!(configured(&both));
    let no_command = GateDefinition {
        rerun_command: String::new(),
        ..both.clone()
    };
    let no_pattern = GateDefinition {
        timeout_pattern: String::new(),
        ..both
    };
    assert!(!configured(&no_command) && !configured(&no_pattern));
}

#[tokio::test]
async fn a_timed_out_test_is_rerun_alone_and_reported_apart_from_failures() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let command = rerun_command("echo ran {test}; test {test} = fast", "fast");
    let (code, _) = managed::gate_command(dir.path(), &command, "fleet-x").await?;
    assert_eq!(code, Some(0));
    let command = rerun_command("test {test} = fast", "slow");
    let (code, _) = managed::gate_command(dir.path(), &command, "fleet-x").await?;
    let reruns = [
        Rerun {
            test: "fast".to_string(),
            passed: true,
        },
        Rerun {
            test: "slow".to_string(),
            passed: code == Some(0),
        },
    ];
    assert_eq!(
        rerun_lines(&reruns),
        vec![
            "- fast timed out in the run and passed alone".to_string(),
            "- slow timed out in the run and failed alone".to_string(),
        ]
    );
    assert!(!only_timeouts_that_pass(&reruns));
    assert!(only_timeouts_that_pass(&reruns[..1]));
    assert!(!only_timeouts_that_pass(&[]));
    Ok(())
}

fn project_gate() -> GateDefinition {
    GateDefinition {
        command: "g".to_string(),
        timeout_pattern: r"TIMEOUT (?P<test>\S+)".to_string(),
        failure_pattern: r"FAIL (?P<test>\S+)".to_string(),
        rerun_command: "t {test}".to_string(),
    }
}

fn alone(test: &str, passed: bool) -> Rerun {
    Rerun {
        test: test.to_string(),
        passed,
    }
}

#[test]
fn a_run_whose_only_failures_timed_out_and_pass_alone_counts_as_passed() {
    let output = "TIMEOUT a::slow\nFAIL a::slow\nTIMEOUT b::slow\n";
    let reruns = [alone("a::slow", true), alone("b::slow", true)];
    assert!(cleared(&project_gate(), output, &reruns));
    assert_eq!(
        unchecked_from("c3", Some(cleared(&project_gate(), output, &reruns))),
        None
    );
}

#[test]
fn a_timed_out_test_that_fails_alone_leaves_the_commit_failed() {
    let output = "TIMEOUT a::slow\nTIMEOUT b::slow\n";
    let reruns = [alone("a::slow", true), alone("b::slow", false)];
    assert!(!cleared(&project_gate(), output, &reruns));
    assert_eq!(
        unchecked_from("c3", Some(cleared(&project_gate(), output, &reruns))),
        Some("c3".to_string())
    );
}

#[test]
fn a_real_failure_beside_a_timeout_leaves_the_commit_failed() {
    let output = "TIMEOUT a::slow\nFAIL c::broken\n";
    let reruns = [alone("a::slow", true)];
    assert!(!cleared(&project_gate(), output, &reruns));
}

#[test]
fn without_a_failure_pattern_a_timeout_never_clears_a_commit() {
    let gate = GateDefinition {
        failure_pattern: String::new(),
        ..project_gate()
    };
    assert!(!cleared(
        &gate,
        "TIMEOUT a::slow\n",
        &[alone("a::slow", true)]
    ));
}
