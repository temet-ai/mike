use chrono::{DateTime, Utc};

use super::build_brake::{
    MEMORY_FLOOR_MB, is_heavy, launched_in_background, parse_available_mb, queued, running_allowed,
    settle_background, waiting_without_wrapper,
};
use super::reuse::{Ask, decide};
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

fn four_builds() -> Vec<TreeResultRow> {
    vec![
        open(1, "2026-10-08T10:00:00Z", "scripts/cx check -p mike --lib"),
        open(2, "2026-10-08T10:01:00Z", "cargo test -p mike"),
        open(3, "2026-10-08T10:02:00Z", "make test-p p=mike"),
        open(4, "2026-10-08T10:03:00Z", "cargo clippy --workspace"),
    ]
}

#[test]
fn heavy_cargo_commands_are_recognised() {
    for command in [
        "cargo build -p mike",
        "CARGO_TARGET_DIR=target/x scripts/cx check -p mike --lib --tests",
        "cargo nextest run -p mike",
        "cargo +nightly clippy --all-targets",
        "make test",
        "scripts/sqlx-prepare --update-changed",
        "cd crates && cargo test 2>&1 | tail",
    ] {
        assert!(is_heavy(command), "{command}");
    }
}

#[test]
fn light_commands_are_not_heavy() {
    for command in [
        "cargo fmt",
        "rg -n check crates",
        "git status",
        "cat Cargo.toml",
        "sleep 3",
        "",
    ] {
        assert!(!is_heavy(command), "{command}");
    }
}

#[test]
fn commands_past_the_slot_count_wait_oldest_first() {
    let waiting = queued(&four_builds(), 2);
    assert_eq!(waiting.into_iter().collect::<Vec<_>>(), vec![3, 4]);
}

#[test]
fn light_open_rows_and_finished_rows_take_no_slot() {
    let mut rows = vec![open(1, "2026-10-08T10:00:00Z", "git status")];
    rows.push(open(2, "2026-10-08T10:01:00Z", "cargo check -p mike"));
    let mut done = open(3, "2026-10-08T10:02:00Z", "cargo check -p mike");
    done.finished_at = Some(at("2026-10-08T10:03:00Z"));
    rows.push(done);
    rows.push(open(4, "2026-10-08T10:04:00Z", "cargo check -p mike"));
    assert!(queued(&rows, 2).is_empty());
    assert_eq!(queued(&rows, 1).into_iter().collect::<Vec<_>>(), vec![4]);
}

#[test]
fn memory_under_the_floor_leaves_only_the_oldest_command_running() {
    let allowed = running_allowed(3, Some(MEMORY_FLOOR_MB - 1), MEMORY_FLOOR_MB);
    assert_eq!(allowed, 1);
    let waiting = queued(&four_builds(), allowed);
    assert_eq!(waiting.into_iter().collect::<Vec<_>>(), vec![2, 3, 4]);
}

#[test]
fn enough_memory_or_unknown_memory_keeps_the_slot_count() {
    assert_eq!(
        running_allowed(3, Some(MEMORY_FLOOR_MB), MEMORY_FLOOR_MB),
        3
    );
    assert_eq!(running_allowed(3, None, MEMORY_FLOOR_MB), 3);
    assert_eq!(running_allowed(0, None, MEMORY_FLOOR_MB), 1);
}

#[test]
fn meminfo_available_is_read_in_megabytes() {
    let text =
        "MemTotal:       32768000 kB\nMemFree:          800000 kB\nMemAvailable:    5836800 kB\n";
    assert_eq!(parse_available_mb(text), Some(5700));
    assert_eq!(parse_available_mb("MemTotal: 1 kB\n"), None);
    assert_eq!(parse_available_mb("MemAvailable: lots kB\n"), None);
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

#[test]
fn only_the_first_heavy_command_runs_when_one_build_directory_is_shared() {
    let waiting = waiting_without_wrapper(&four_builds());
    assert_eq!(waiting.into_iter().collect::<Vec<_>>(), vec![2, 3, 4]);
}

#[test]
fn a_command_with_its_own_target_directory_neither_waits_nor_takes_the_slot() {
    let rows = vec![
        open(1, "2026-10-08T10:00:00Z", "scripts/cx check -p mike --lib"),
        open(
            2,
            "2026-10-08T10:01:00Z",
            "CARGO_TARGET_DIR=target/t8 scripts/cx check -p mike --lib",
        ),
        open(3, "2026-10-08T10:02:00Z", "cargo test -p mike"),
    ];
    assert_eq!(queued(&rows, 1).into_iter().collect::<Vec<_>>(), vec![3]);
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
fn completed_and_unconfirmed_background_builds_do_not_imply_live_slot_occupancy() {
    let rows = vec![
        bg(1, "2026-10-08T10:00:00Z", "scripts/cx check -p mike --lib"),
        bg(2, "2026-10-08T10:00:30Z", "scripts/cx nextest run -p mike"),
        bg(3, "2026-10-08T10:01:00Z", "scripts/cx clippy -p mike"),
    ];
    let ends = [end(1, "completed", Some("2026-10-08T10:02:00Z"))];
    let settled = settle_background(rows, &ends, at("2026-10-08T10:05:00Z"));
    assert!(queued(&settled, 1).is_empty());
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
fn a_fleet_with_no_wrapper_reports_the_third_build_as_waiting() {
    let rows = four_builds();
    let ask = Ask {
        command: "make test-p p=mike",
        wrappers: &[],
        slots: 2,
    };
    let answer = decide(&ask, &rows, &[]);
    assert_eq!(answer.answer, "waiting", "{answer:?}");
    let first = Ask {
        command: "scripts/cx check -p mike --lib",
        wrappers: &[],
        slots: 2,
    };
    assert_ne!(decide(&first, &rows, &[]).answer, "waiting");
}

#[test]
fn commands_sharing_an_explicit_target_directory_are_inferred_to_queue_together() {
    let rows = vec![
        open(
            1,
            "2026-10-08T10:00:00Z",
            "CARGO_TARGET_DIR=target/fleet cargo check -p a",
        ),
        open(
            2,
            "2026-10-08T10:01:00Z",
            "env CARGO_TARGET_DIR=target/fleet timeout 580 cargo test -p b",
        ),
        open(
            3,
            "2026-10-08T10:02:00Z",
            "cargo test -p c --target-dir=./target/fleet/",
        ),
    ];
    assert_eq!(queued(&rows, 1).into_iter().collect::<Vec<_>>(), vec![2, 3]);
    assert_eq!(queued(&rows, 2).into_iter().collect::<Vec<_>>(), vec![3]);
}

#[test]
fn each_literal_target_directory_has_its_own_inferred_queue() {
    let rows = vec![
        open(
            1,
            "2026-10-08T10:00:00Z",
            "cargo check --target-dir target/a",
        ),
        open(
            2,
            "2026-10-08T10:01:00Z",
            "CARGO_TARGET_DIR=target/b cargo test",
        ),
        open(
            3,
            "2026-10-08T10:02:00Z",
            "CARGO_TARGET_DIR=target/a cargo test",
        ),
        open(
            4,
            "2026-10-08T10:03:00Z",
            "cargo check --target-dir=target/b",
        ),
    ];
    assert_eq!(queued(&rows, 1).into_iter().collect::<Vec<_>>(), vec![3, 4]);
}

#[test]
fn changing_to_separate_worktrees_or_unknown_targets_does_not_imply_shared_lock_wait() {
    let rows = vec![
        open(1, "2026-10-08T10:00:00Z", "cargo check"),
        open(
            2,
            "2026-10-08T10:01:00Z",
            "cd .claude/worktrees/other && cargo check",
        ),
        open(
            3,
            "2026-10-08T10:02:00Z",
            "CARGO_TARGET_DIR=$TASK_TARGET cargo check",
        ),
        open(4, "2026-10-08T10:03:00Z", "cargo check --target-dir target"),
    ];
    assert_eq!(queued(&rows, 1).into_iter().collect::<Vec<_>>(), vec![4]);
}

#[test]
fn a_build_queue_answer_labels_its_inference_instead_of_claiming_a_process_state() {
    let rows = four_builds();
    let ask = Ask {
        command: "cargo test -p mike",
        wrappers: &[],
        slots: 1,
    };
    let answer = decide(&ask, &rows, &[]);
    assert_eq!(answer.answer, "waiting");
    assert!(answer.message.contains("this is inferred"));
    assert!(!answer.message.contains("not running"));
}
