use chrono::{DateTime, Utc};

use super::background_result::{launched_in_background, settle_background};
use super::operation::{CountedResource, Execution, ResourceClaim, TimedCommand};
use super::reuse::{Ask, decide, waiting_uses};
use crate::store::persistence::fleet::{BackgroundEnd, TreeResultRow};

fn at(text: &str) -> DateTime<Utc> {
    text.parse().unwrap_or_default()
}

fn open(id: i64, started: &str, command: &str) -> TreeResultRow {
    TreeResultRow {
        use_id: id,
        seat: Some(format!("seat-{id}")),
        session_uuid: None,
        command: Some(command.into()),
        started_at: Some(at(started)),
        finished_at: None,
        is_error: None,
        content: None,
    }
}

fn shared(capacity: u32) -> Vec<ResourceClaim> {
    vec![ResourceClaim::Counted(CountedResource {
        name: "shared-build".into(),
        capacity,
    })]
}

fn declared(capacity: u32) -> Execution {
    let cargo = TimedCommand {
        subcommands: ["check", "test", "clippy", "nextest"]
            .map(String::from)
            .to_vec(),
        resources: shared(capacity),
        through: vec!["scripts/cx".into()],
    };
    let make = TimedCommand {
        resources: shared(capacity),
        ..TimedCommand::default()
    };
    Execution {
        timed_commands: [("cargo".to_string(), cargo), ("make".to_string(), make)]
            .into_iter()
            .collect(),
        ..Execution::default()
    }
}

fn four_builds() -> Vec<TreeResultRow> {
    vec![
        open(1, "2026-10-08T10:00:00Z", "scripts/cx check -p mike --lib"),
        open(2, "2026-10-08T10:01:00Z", "cargo test -p mike"),
        open(3, "2026-10-08T10:02:00Z", "make test-p p=mike"),
        open(4, "2026-10-08T10:03:00Z", "cargo clippy --workspace"),
    ]
}

fn waiting(rows: &[TreeResultRow], capacity: u32) -> Vec<i64> {
    waiting_uses(rows, &[], &declared(capacity))
        .into_iter()
        .collect()
}

#[test]
fn commands_past_a_declared_capacity_wait_oldest_first() {
    assert_eq!(waiting(&four_builds(), 2), vec![3, 4]);
    assert_eq!(waiting(&four_builds(), 1), vec![2, 3, 4]);
}

#[test]
fn a_project_that_declares_no_shared_resource_has_no_inferred_queue() {
    let rows = four_builds();
    assert!(waiting_uses(&rows, &[], &Execution::default()).is_empty());
    let timed_only = Execution {
        timed_commands: [("cargo".to_string(), TimedCommand::default())]
            .into_iter()
            .collect(),
        ..Execution::default()
    };
    assert!(waiting_uses(&rows, &[], &timed_only).is_empty());
}

#[test]
fn undeclared_open_rows_and_finished_rows_take_no_unit() {
    let mut rows = vec![open(1, "2026-10-08T10:00:00Z", "git status")];
    rows.push(open(2, "2026-10-08T10:01:00Z", "cargo check -p mike"));
    rows.push(open(3, "2026-10-08T10:01:30Z", "cargo metadata"));
    let mut done = open(4, "2026-10-08T10:02:00Z", "cargo check -p mike");
    done.finished_at = Some(at("2026-10-08T10:03:00Z"));
    rows.push(done);
    rows.push(open(5, "2026-10-08T10:04:00Z", "cargo check -p mike"));
    assert!(waiting(&rows, 2).is_empty());
    assert_eq!(waiting(&rows, 1), vec![5]);
}

#[test]
fn a_definition_wrapper_counts_as_running_the_timed_program() {
    let rows = vec![
        open(1, "2026-10-08T10:00:00Z", "scripts/slot cargo test -p a"),
        open(2, "2026-10-08T10:01:00Z", "scripts/slot cargo test -p b"),
    ];
    let wrappers = vec!["scripts/slot".to_string()];
    let found = waiting_uses(&rows, &wrappers, &declared(1));
    assert_eq!(found.into_iter().collect::<Vec<_>>(), vec![2]);
}

fn background_reply(
    id: i64,
    started: &str,
    command: &str,
    text: serde_json::Value,
) -> TreeResultRow {
    let mut row = open(id, started, command);
    row.finished_at = Some(at(started) + chrono::Duration::milliseconds(100));
    row.is_error = Some(false);
    row.content = Some(text);
    row
}

fn end(use_id: i64, state: &str, finished: Option<&str>) -> BackgroundEnd {
    BackgroundEnd {
        use_id,
        state: state.into(),
        finished_at: finished.map(at),
    }
}

fn bg(id: i64, started: &str, command: &str) -> TreeResultRow {
    background_reply(
        id,
        started,
        command,
        serde_json::json!(
            "Command running in background with ID: b1. Output is being written to: /x"
        ),
    )
}

#[test]
fn a_background_launch_never_ages_into_a_success_without_completion() {
    let rows = vec![bg(
        1,
        "2026-10-08T10:00:00Z",
        "scripts/cx nextest run -p mike",
    )];
    let soon = settle_background(rows.clone(), &[], at("2026-10-08T10:20:00Z"));
    assert!(soon[0].finished_at.is_none());
    assert!(soon[0].is_error.is_none());
    let late = settle_background(rows, &[], at("2026-10-08T11:00:00Z"));
    assert!(late[0].finished_at.is_none());
    assert!(late[0].is_error.is_none());
}

#[test]
fn a_background_row_whose_end_was_captured_is_finished_at_that_end() {
    let rows = vec![bg(
        1,
        "2026-10-08T10:00:00Z",
        "scripts/cx nextest run -p mike",
    )];
    let ends = [end(1, "completed", Some("2026-10-08T10:03:00Z"))];
    let settled = settle_background(rows, &ends, at("2026-10-08T10:04:00Z"));
    assert_eq!(settled[0].finished_at, Some(at("2026-10-08T10:03:00Z")));
    assert_eq!(settled[0].is_error, Some(false));
}

#[test]
fn a_failed_or_killed_background_command_is_finished_as_an_error() {
    let rows = vec![
        bg(1, "2026-10-08T10:00:00Z", "cargo test"),
        bg(2, "2026-10-08T10:00:00Z", "cargo test"),
    ];
    let ends = [
        end(1, "failed", Some("2026-10-08T10:01:00Z")),
        end(2, "killed", Some("2026-10-08T10:02:00Z")),
    ];
    let settled = settle_background(rows, &ends, at("2026-10-08T10:04:00Z"));
    assert!(settled.iter().all(|r| r.is_error == Some(true)));
    assert!(settled.iter().all(|r| r.finished_at.is_some()));
}

#[test]
fn a_background_command_the_ledger_still_calls_running_stays_open() {
    let rows = vec![bg(1, "2026-10-08T10:00:00Z", "cargo test")];
    let ends = [end(1, "running", None)];
    let settled = settle_background(rows, &ends, at("2026-10-08T10:10:00Z"));
    assert!(settled[0].finished_at.is_none());
}

#[test]
fn completed_and_unconfirmed_background_builds_do_not_imply_live_occupancy() {
    let rows = vec![
        bg(1, "2026-10-08T10:00:00Z", "scripts/cx check -p mike --lib"),
        bg(2, "2026-10-08T10:00:30Z", "scripts/cx nextest run -p mike"),
        bg(3, "2026-10-08T10:01:00Z", "scripts/cx clippy -p mike"),
    ];
    let ends = [end(1, "completed", Some("2026-10-08T10:02:00Z"))];
    let settled = settle_background(rows, &ends, at("2026-10-08T10:05:00Z"));
    assert!(waiting(&settled, 1).is_empty());
}

#[test]
fn background_reply_in_text_blocks_is_recognised_and_ordinary_output_is_left_alone() {
    let blocks =
        serde_json::json!([{"type": "text", "text": "Command running in background with ID: b1"}]);
    let reply = background_reply(1, "2026-10-08T10:00:00Z", "cargo test", blocks);
    assert!(launched_in_background(&reply));
    let ordinary = background_reply(
        2,
        "2026-10-08T10:00:00Z",
        "cargo test",
        serde_json::json!("test result: ok. 3 passed"),
    );
    assert!(!launched_in_background(&ordinary));
    let kept = settle_background(vec![ordinary], &[], at("2026-10-08T10:01:00Z"));
    assert!(kept[0].finished_at.is_some());
}

#[test]
fn a_queue_answer_labels_its_inference_instead_of_claiming_a_process_state() {
    let rows = four_builds();
    let execution = declared(1);
    let ask = Ask {
        command: "cargo test -p mike",
        wrappers: &[],
        execution: &execution,
    };
    let answer = decide(&ask, &rows, &[]);
    assert_eq!(answer.answer, "waiting");
    assert!(answer.message.contains("this is inferred"));
    assert!(!answer.message.contains("not running"));
    let first = Ask {
        command: "scripts/cx check -p mike --lib",
        ..ask
    };
    assert_ne!(decide(&first, &rows, &[]).answer, "waiting");
}
