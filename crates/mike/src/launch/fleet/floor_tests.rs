use uuid::Uuid;

use super::{
    Evidence, FailCode, Observed, Outcome, evaluate, event, parse_name_status, seat_answer, under,
};
use crate::launch::fleet::definition::{DoneCheck, DoneExercise, DoneWhen};
use crate::launch::fleet::events::{EventKind, EventPayload};

const SHA: &str = "abc123";

fn done() -> DoneWhen {
    DoneWhen {
        checks: vec![DoneCheck {
            operation: "unit".into(),
            expected_exit: 0,
        }],
        tests_added_under: vec!["crates/x/tests".into()],
        files_changed: vec!["crates/x/src/lib.rs".into()],
        exercise: Some(DoneExercise {
            command: "run-it".into(),
            expect_output: "drawn".into(),
        }),
    }
}

fn seen(exit: Option<i32>, output: &str) -> Observed {
    Observed {
        operation: "unit".into(),
        command: "cargo nextest run -p x --lib store::".into(),
        exit,
        output: output.into(),
    }
}

fn good() -> Evidence {
    Evidence {
        checks: vec![seen(Some(0), "ok")],
        touched: vec!["crates/x/src/lib.rs".into()],
        added: vec!["crates/x/tests/new.rs".into()],
        exercise: Some(seen(Some(0), "frame drawn by Ink")),
    }
}

fn failure(evidence: &Evidence) -> (FailCode, String) {
    match evaluate(&done(), SHA, evidence) {
        Outcome::Failed { code, line, .. } => (code, line),
        Outcome::Passed { .. } => panic!("the floor should fail"),
    }
}

#[test]
fn a_floor_with_every_line_met_passes_and_reaches_the_judge() {
    let outcome = evaluate(&done(), SHA, &good());
    assert_eq!(outcome, Outcome::Passed { sha: SHA.into() });
    assert!(outcome.reaches_judge());
    assert_eq!(seat_answer("T1", &outcome), None);
}

#[test]
fn a_failing_check_records_floor_failed_and_no_judge_reads_the_report() {
    let mut evidence = good();
    evidence.checks = vec![seen(Some(101), "line one\nthe failing assertion")];
    let outcome = evaluate(&done(), SHA, &evidence);
    assert!(!outcome.reaches_judge());
    let seat = Uuid::from_u128(7);
    let recorded = event("run", "T1", seat, &outcome);
    assert_eq!(recorded.payload.kind().ok(), Some(EventKind::FloorFailed));
    let EventPayload::FloorFailed(failed) = recorded.payload else {
        panic!("floor_failed expected");
    };
    assert_eq!(failed.sha, SHA);
    assert!(
        failed.check.contains("the failing assertion"),
        "{}",
        failed.check
    );
    assert!(failed.check.contains("observed 101"), "{}", failed.check);
    let answer = seat_answer("T1", &outcome).unwrap_or_default();
    assert!(answer.contains("check_failed"), "{answer}");
    assert!(answer.contains("No judge has read this report"), "{answer}");
}

#[test]
fn a_passing_floor_records_floor_passed() {
    let outcome = evaluate(&done(), SHA, &good());
    let recorded = event("run", "T1", Uuid::nil(), &outcome);
    assert_eq!(recorded.payload.kind().ok(), Some(EventKind::FloorPassed));
    assert_eq!(recorded.task.as_deref(), Some("T1"));
}

#[test]
fn each_unmet_line_fails_with_its_own_code_and_names_the_line() {
    let mut evidence = good();
    evidence.touched = vec!["crates/x/src/other.rs".into()];
    let (code, line) = failure(&evidence);
    assert_eq!(code, FailCode::FilesChanged);
    assert!(line.contains("crates/x/src/lib.rs"), "{line}");

    let mut evidence = good();
    evidence.added.clear();
    let (code, line) = failure(&evidence);
    assert_eq!(code, FailCode::TestsAdded);
    assert!(line.contains("crates/x/tests"), "{line}");

    let mut evidence = good();
    evidence.exercise = Some(seen(Some(0), "nothing on screen"));
    let (code, line) = failure(&evidence);
    assert_eq!(code, FailCode::Exercise);
    assert!(line.contains("drawn"), "{line}");

    let mut evidence = good();
    evidence.exercise = Some(seen(None, "could not start"));
    assert_eq!(failure(&evidence).0, FailCode::Exercise);
}

#[test]
fn a_check_that_never_ran_fails_the_floor() {
    let mut evidence = good();
    evidence.checks = vec![seen(None, "no clean checkout")];
    let (code, line) = failure(&evidence);
    assert_eq!(code, FailCode::Check);
    assert!(line.contains("observed none"), "{line}");
}

#[test]
fn a_nextest_filter_that_ran_no_test_is_a_definition_problem_not_a_task_failure() {
    let mut evidence = good();
    evidence.checks = vec![seen(
        Some(1),
        "error: no tests to run\n[mike ruling] observed exit: 4; expected: 0\n",
    )];
    let (code, line) = failure(&evidence);
    assert_eq!(code, FailCode::Definition);
    assert!(
        line.contains("matched no test under this profile"),
        "{line}"
    );
    let outcome = evaluate(&done(), SHA, &evidence);
    let answer = seat_answer("T1", &outcome).unwrap_or_default();
    assert!(answer.contains("Your work is not failed"), "{answer}");
    assert!(answer.contains("tasks.T1.done_when"), "{answer}");
    let mut real = good();
    real.checks = vec![seen(
        Some(1),
        "test a ... FAILED\n[mike ruling] observed exit: 100; expected: 0\n",
    )];
    assert_eq!(failure(&real).0, FailCode::Check);
}

#[test]
fn a_path_is_under_a_prefix_only_at_a_directory_boundary() {
    assert!(under("crates/x/tests/a.rs", "crates/x/tests"));
    assert!(under("crates/x/tests/a.rs", "crates/x/tests/"));
    assert!(under("crates/x/src/lib.rs", "crates/x/src/lib.rs"));
    assert!(!under("crates/x/tests_old/a.rs", "crates/x/tests"));
    assert!(!under("crates/x/tests/a.rs", ""));
}

#[test]
fn whole_crate_and_full_suite_floor_commands_are_named_heavy() {
    for heavy in [
        "scripts/cx nextest run --profile ci -p mike",
        "scripts/cx nextest run --profile ci -p mike --lib",
        "cargo test -p mike --lib",
        "scripts/gate --json check",
        "make check",
        "make test-p p=mike",
        "cargo clippy --workspace --all-targets",
        "scripts/cx nextest run -p mike-mcp && echo done",
    ] {
        assert!(super::heavy_reason(heavy).is_some(), "{heavy}");
    }
    for focused in [
        "scripts/cx nextest run --profile ci -p mike --lib launch::fleet::floor",
        "cargo test -p mike --lib -- floor",
        "scripts/cx nextest run -p mike -E 'test(floor)'",
        "make test-t t=launch::fleet",
        "rustfmt --check --edition 2024 crates/mike/src/x.rs",
        "CARGO_TARGET_DIR=target/fleet-check scripts/cx check -p mike --lib --tests",
    ] {
        assert_eq!(super::heavy_reason(focused), None, "{focused}");
    }
}

#[test]
fn define_fleet_warns_about_a_floor_that_runs_a_whole_crate_and_not_a_focused_one() {
    let def = crate::launch::fleet::tests::changed(serde_json::json!({
        "execution": {"operations": {
            "all-mike": {"command": "scripts/cx nextest run --profile ci -p mike", "purpose": "every mike test"},
            "floor-tests": {"command": "scripts/cx nextest run --profile ci -p mike --lib launch::fleet::floor", "purpose": "the floor tests"}
        }},
        "tasks": {
            "T1": {"done_when": {"checks": [{"operation": "all-mike", "expected_exit": 0}]}},
            "T2": {"done_when": {"checks": [{"operation": "floor-tests", "expected_exit": 0}]}}
        }
    }));
    let warned = super::heavy_floor_warnings(&def);
    let items: Vec<&str> = warned.iter().map(|w| w.item.as_str()).collect();
    assert_eq!(items, vec!["tasks.T1.done_when"], "{warned:?}");
    assert!(
        warned[0].problem.contains("every test of a crate"),
        "{warned:?}"
    );
}

#[test]
fn commit_files_are_read_with_their_status() {
    let (touched, added) = parse_name_status("M\ta/b.rs\nA\ta/new_tests.rs\n\nD\ta/gone.rs\n");
    assert_eq!(touched, vec!["a/b.rs", "a/new_tests.rs", "a/gone.rs"]);
    assert_eq!(added, vec!["a/new_tests.rs"]);
}
