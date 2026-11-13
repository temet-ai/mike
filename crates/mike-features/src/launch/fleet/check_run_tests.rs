#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::path::Path;

use crate::launch::fleet::managed_test_support as managed;

#[tokio::test]
async fn a_clean_check_and_a_gate_queue_on_their_own_checkouts_not_the_repository() {
    let repo = tempfile::tempdir().expect("repo");
    let path = repo.path().to_string_lossy().into_owned();
    let check = super::checkout_used(&path, true);
    let gate = crate::launch::fleet::gate_slot::slot_for(&path).expect("gate slot");
    assert!(check.ends_with(".claude/worktrees/mike-check"), "{check:?}");
    assert_ne!(check, gate);
    assert_ne!(check, repo.path());
    assert_eq!(super::checkout_used(&path, false), repo.path());
    let _gate = crate::launch::fleet::gate_slot::queue::hold(&gate).await;
    let ruling = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        crate::launch::fleet::gate_slot::queue::hold(&check),
    )
    .await;
    assert!(
        ruling.is_ok(),
        "a running gate must not hold up a clean check"
    );
}

use serde_json::json;
use tokio_util::task::TaskTracker;
use uuid::Uuid;

use super::{base_run, focus, focused_command, is_focused, queue, target};
use crate::launch::fleet::check::{Runner, Verdict, head, state_at};
use crate::launch::fleet::check_proof::{Landed, Proof, draft, parse_landed};
use crate::launch::fleet::definition::FleetDefinition;
use crate::launch::fleet::diagnostics::Diagnostic;
use crate::launch::fleet::events::{EventPayload, NewFleetEvent, SeatLaunchConfirmed};
use crate::launch::fleet::tests::{complete, parsed};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::TreeCheckRow;
use crate::store::persistence::session_notes::{NoteOrder, SessionNoteFilter};
use crate::store::persistence::test_support::acquire_ephemeral_db;

const SEAT_SESSION: u128 = 51;
const CHECK: &str = "grep -q ok mine/a.txt || { echo 'mine/a.txt:1: not ok'; exit 1; }";

fn seated() -> crate::launch::fleet::seated::Seated {
    crate::launch::fleet::seated::Seated::default().worker(
        "M",
        Uuid::from_u128(SEAT_SESSION),
        "db-dev-M",
    )
}

async fn seat_m(persist: &PersistHandle, run: &str) -> anyhow::Result<()> {
    persist
        .fleet()
        .append_fleet_event(&NewFleetEvent {
            run_id: run.to_string(),
            task: Some("M".to_string()),
            session_uuid: Some(Uuid::from_u128(SEAT_SESSION)),
            payload: EventPayload::SeatLaunchConfirmed(SeatLaunchConfirmed { launch_id: 1 }),
        })
        .await?;
    Ok(())
}

pub async fn git(dir: &Path, args: &[&str]) -> anyhow::Result<String> {
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

async fn seat_commit(repo: &Path, file: &str, text: &str) -> anyhow::Result<String> {
    std::fs::write(repo.join(file), text)?;
    git(repo, &["add", file]).await?;
    let message = format!(
        "edit {file}\n\nFleet-Seat: db-dev-M ({})\n",
        Uuid::from_u128(SEAT_SESSION)
    );
    git(repo, &["commit", "-q", "-m", &message]).await?;
    git(repo, &["rev-parse", "HEAD"]).await
}

pub async fn project(root: &Path) -> anyhow::Result<(std::path::PathBuf, FleetDefinition)> {
    let repo = root.join("project");
    std::fs::create_dir_all(repo.join("mine"))?;
    std::fs::create_dir_all(repo.join("theirs"))?;
    git(&repo, &["init", "-q", "-b", "main"]).await?;
    git(&repo, &["config", "user.email", "t@example.com"]).await?;
    git(&repo, &["config", "user.name", "t"]).await?;
    std::fs::write(repo.join("theirs/b.txt"), "fine\n")?;
    git(&repo, &["add", "theirs/b.txt"]).await?;
    seat_commit(&repo, "mine/a.txt", "ok\n").await?;
    let mut value = complete();
    value["repo"] = json!(repo.to_string_lossy());
    value["check"] = json!({"command": CHECK, "paths": ["mine", "theirs"]});
    value["execution"] = json!({"operations": {"fixture": {"command": CHECK, "purpose": "fixture content", "inputs": ["mine", "theirs"], "cache_key": "fixture-v1"}}});
    value["tasks"] = json!({
        "M": {"name": "mine", "discipline": "db-dev", "criteria": ["C1"], "files": ["mine"],
              "brief": "x"},
        "T": {"name": "theirs", "discipline": "db-dev", "criteria": ["C2"], "files": ["theirs"],
              "brief": "x"}
    });
    value["shared_files"] = json!([]);
    Ok((repo, parsed(&value)))
}

#[tokio::test]
async fn configured_check_isolates_committed_source_while_explicit_commands_read_peer_edits()
-> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let (repo, def) = project(root.path()).await?;
    let at = head(&def).await.map_err(anyhow::Error::msg)?;
    assert_eq!(at.slot, repo.join(".claude/worktrees/mike-check"));
    let (code, output) = managed::check(&def, &at, "fleet-shared").await?;
    assert_eq!((code, output.as_str()), (Some(0), ""), "{output}");

    std::fs::write(repo.join("mine/a.txt"), "half an edi")?;
    std::fs::write(repo.join("theirs/new.txt"), "a peer's file, not committed")?;
    let (code, output) = managed::check(&def, &at, "fleet-shared").await?;
    assert_eq!((code, output.as_str()), (Some(0), ""));
    let (code, output) = managed::check(&def, &at, "fleet-shared#command:custom").await?;
    assert_eq!((code, output.as_str()), (Some(1), "mine/a.txt:1: not ok\n"));
    assert_eq!(
        std::fs::read_to_string(repo.join("mine/a.txt"))?,
        "half an edi"
    );
    assert!(repo.join("theirs/new.txt").exists());
    assert!(!at.slot.join("theirs/new.txt").exists());
    Ok(())
}

#[tokio::test]
async fn the_state_of_a_commit_ignores_uncommitted_edits_and_files_outside_the_checks_paths()
-> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let (repo, def) = project(root.path()).await?;
    let first = state_at(&def.repo, &def.check, "HEAD")
        .await
        .map_err(anyhow::Error::msg)?;
    std::fs::write(repo.join("mine/a.txt"), "half an edi")?;
    let edited = state_at(&def.repo, &def.check, "HEAD")
        .await
        .map_err(anyhow::Error::msg)?;
    assert_eq!(first, edited, "an uncommitted edit is not in any state");
    seat_commit(&repo, "notes.md", "docs\n").await?;
    let docs = state_at(&def.repo, &def.check, "HEAD")
        .await
        .map_err(anyhow::Error::msg)?;
    assert_eq!(
        first, docs,
        "a commit outside the check's paths keeps its verdict"
    );
    seat_commit(&repo, "mine/a.txt", "ok again\n").await?;
    let code = state_at(&def.repo, &def.check, "HEAD")
        .await
        .map_err(anyhow::Error::msg)?;
    assert_ne!(first, code);
    let missing = state_at(&def.repo, &def.check, "no-such-commit").await;
    assert!(
        missing.is_err_and(|why| why.contains("could not be read")),
        "an unknown commit names no state"
    );
    Ok(())
}

#[test]
fn landed_commits_are_read_with_the_session_their_trailer_names() {
    let session = Uuid::from_u128(7);
    let raw = format!(
        "aaaa\u{1f}db-dev-M ({session})\n\u{1e}\nbbbb\u{1f}\u{1e}\ncccc\u{1f}someone without a session\u{1e}\n"
    );
    assert_eq!(
        parse_landed(&raw),
        vec![Landed {
            sha: "aaaa".into(),
            session
        }]
    );
}

fn verdict(exit: Option<i32>, output: &str) -> Verdict {
    Verdict {
        row: TreeCheckRow {
            id: 9,
            run_id: "r".into(),
            state: "abcdef0123456789".into(),
            state_after: Some("abcdef0123456789".into()),
            command: "make check".into(),
            started_at: chrono::Utc::now(),
            finished_at: Some(chrono::Utc::now()),
            exit_code: exit,
            output: output.into(),
            askers: vec!["mike".into()],
            told: vec![],
        },
        commit: "c0ffee0000c0ffee0000".into(),
        how: "finished".into(),
    }
}

#[test]
fn the_proof_note_names_each_commit_and_whether_the_check_proved_it() {
    let proofs = [
        Proof {
            sha: "1111".into(),
            proven: false,
        },
        Proof {
            sha: "2222".into(),
            proven: true,
        },
    ];
    let note = draft(("M", "db-dev-M"), &verdict(Some(0), ""), &proofs, &[]);
    assert_eq!(
        note.title,
        "M: `make check` passed at commit c0ffee0000; 1 of 2 commit(s) proven"
    );
    assert!(note.body.contains("- 2222 proven"), "{}", note.body);
    assert!(note.body.contains("- 1111 not proven"), "{}", note.body);
    assert_eq!(
        note.data["commits"][1],
        json!({"sha": "2222", "proven": true})
    );
    assert_eq!(note.data["checked_commit"], json!("c0ffee0000c0ffee0000"));

    let mine = [Diagnostic {
        path: "mine/a.txt".into(),
        line: 1,
        text: "mine/a.txt:1: not ok".into(),
    }];
    let failed = draft(
        ("M", "db-dev-M"),
        &verdict(Some(1), "mine/a.txt:1: not ok"),
        &proofs[1..],
        &mine,
    );
    assert_eq!(
        failed.title,
        "M: `make check` failed at commit c0ffee0000, in this task's files"
    );
    assert!(
        failed
            .body
            .contains("Errors in this task's files:\n- mine/a.txt:1")
    );
    assert_eq!(failed.data["mine"][0]["line"], json!(1));
}

async fn results(persist: &PersistHandle) -> anyhow::Result<Vec<String>> {
    let filter = SessionNoteFilter {
        kind: Some("result"),
        status: None,
        tag: Some("check"),
        q: None,
        order: NoteOrder::NewestFirst,
        limit: 20,
        offset: 0,
    };
    Ok(persist
        .launch()
        .fetch_session_notes(Uuid::from_u128(SEAT_SESSION), filter)
        .await?
        .into_iter()
        .map(|n| n.title)
        .collect())
}

async fn inbox(persist: &PersistHandle) -> anyhow::Result<Vec<String>> {
    Ok(persist
        .launch()
        .fetch_for_session_with_activity(Uuid::from_u128(SEAT_SESSION))
        .await?
        .inbox
        .into_iter()
        .map(|m| m.row.body)
        .collect())
}

async fn queued_check(
    runner: &Runner<'_>,
    def: &FleetDefinition,
    tracker: &TaskTracker,
    persist: &PersistHandle,
    run: &str,
) -> anyhow::Result<TreeCheckRow> {
    tracker.reopen();
    Box::pin(queue(runner, def)).await;
    tracker.close();
    tracker.wait().await;
    persist
        .fleet()
        .latest_tree_check(run)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no check was recorded"))
}

#[test]
fn a_test_name_goes_into_the_projects_command_as_one_quoted_word() {
    assert_eq!(
        focused_command("run-one {test} --quiet", "mod::case"),
        Ok("run-one 'mod::case' --quiet".to_string())
    );
    assert_eq!(
        focused_command("run-one --filter", "  mike::cli x  "),
        Ok("run-one --filter 'mike::cli x'".to_string())
    );
    assert_eq!(
        focused_command("t {test} {test}", "it's"),
        Ok("t 'it'\\''s' 'it'\\''s'".to_string())
    );
}

#[test]
fn a_test_name_that_is_empty_or_spans_lines_or_has_no_project_command_is_refused() {
    assert!(focused_command("run {test}", "   ").is_err_and(|why| why.contains("one test")));
    assert!(focused_command("run {test}", "a\nrm -rf x").is_err());
    assert!(focused_command("  ", "a").is_err_and(|why| why.contains("check.test_command")));
}

#[test]
fn a_focused_run_has_its_own_ledger_and_command_and_keeps_the_fleets_check_untouched() {
    let mut def = parsed(&complete());
    def.check.command = "full".to_string();
    def.check.test_command = "one {test}".to_string();
    let focused = focus(&def, "fleet-run-1", "a::b").expect("a declared command focuses");
    assert_eq!(focused.ledger, "fleet-run-1#test:a::b");
    assert_eq!(focused.def.check.command, "one 'a::b'");
    assert_eq!(def.check.command, "full");
    assert_eq!(base_run(&focused.ledger), "fleet-run-1");
    assert!(is_focused(&focused.ledger) && !is_focused("fleet-run-1"));
    assert!(focus(&def, "fleet-run-1", "").is_err());
}

#[tokio::test]
async fn a_focused_run_executes_only_the_named_test_on_its_captured_commit() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let (repo, mut def) = project(root.path()).await?;
    def.check.test_command = "grep -q {test} mine/a.txt".to_string();
    let first = git(&repo, &["rev-parse", "HEAD"]).await?;
    seat_commit(&repo, "mine/a.txt", "changed\n").await?;

    let newest = target(&def, None).await.map_err(anyhow::Error::msg)?;
    let named = target(&def, Some(&newest.commit[..12]))
        .await
        .map_err(anyhow::Error::msg)?;
    assert_eq!(named.commit, newest.commit);
    let older = target(&def, Some(&first[..12]))
        .await
        .map_err(anyhow::Error::msg)?;
    assert_eq!(older.commit, first);
    assert_ne!(older.slot, repo);

    let run = |test: &'static str| focus(&def, "fleet-focus", test).map_err(anyhow::Error::msg);
    let (code, _) = managed::check(&run("ok")?.def, &newest, "fleet-focus#test:ok").await?;
    assert_eq!(code, Some(1), "the newest commit no longer does");
    let (code, _) =
        managed::check(&run("changed")?.def, &newest, "fleet-focus#test:changed").await?;
    assert_eq!(code, Some(0));
    let (code, output) = managed::check(
        &run("ok")?.def,
        &older,
        "fleet-focus#test:ok#baseline:first",
    )
    .await?;
    assert_eq!(code, Some(0), "{output}");
    assert_eq!(
        std::fs::read_to_string(repo.join("mine/a.txt"))?,
        "changed\n"
    );
    Ok(())
}

#[tokio::test]
async fn a_commit_that_is_not_one_or_looks_like_an_option_is_refused() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let (_, def) = project(root.path()).await?;
    let missing = target(&def, Some("no-such-commit")).await;
    assert!(missing.is_err_and(|why| why.contains("not a commit")));
    let option = target(&def, Some("--output=/tmp/x")).await;
    assert!(option.is_err_and(|why| why.contains("not a commit")));
    assert!(target(&def, Some("  ")).await.is_ok());
    Ok(())
}

#[path = "check_baseline_tests.rs"]
pub mod baseline_tests;

#[path = "ruling_exec_tests.rs"]
pub mod ruling_tests;

#[path = "check_queue_tests.rs"]
pub mod queue_tests;

#[path = "check_source_tests.rs"]
pub mod source_tests;

#[path = "check_isolation_tests.rs"]
pub mod isolation_tests;

#[path = "check_run_commit_tests.rs"]
pub mod commit_tests;

#[tokio::test]
async fn two_rulings_run_at_once_in_two_check_slots() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let (_repo, mut def) = project(root.path()).await?;
    let at = head(&def).await.map_err(anyhow::Error::msg)?;
    let meeting = root.path().join("meeting");
    std::fs::create_dir_all(&meeting)?;
    def.check.command = format!(
        "m='{}'; touch \"$m/$MIKE_RUN_ID\"; for i in $(seq 200); do [ \"$(ls \"$m\" | wc -l)\" -ge 2 ] && exit 0; sleep 0.1; done; exit 1",
        meeting.display()
    );
    let (first, second) = tokio::join!(
        managed::check(&def, &at, "fleet-ruling-a"),
        managed::check(&def, &at, "fleet-ruling-b")
    );
    assert_eq!(first?.0, Some(0));
    assert_eq!(second?.0, Some(0));
    Ok(())
}
