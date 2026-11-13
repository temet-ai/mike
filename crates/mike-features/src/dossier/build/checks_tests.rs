use chrono::{DateTime, Duration, Utc};

use super::checks::{
    BashResult, BashUse, RegistryRow, check_kind, derive_checks, failed_count, parse_exit_code,
};

fn at(minute: i64) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp(1_790_000_000, 0).unwrap_or_default()
        + Duration::minutes(minute)
}

fn result(content: &str, is_error: bool) -> Option<BashResult> {
    Some(BashResult {
        content: content.to_string(),
        is_error,
    })
}

fn usage(command: &str, result: Option<BashResult>) -> BashUse {
    BashUse {
        tool_use_id: "toolu_1".to_string(),
        request_id: 10,
        at: at(1),
        command: command.to_string(),
        result,
        registry: None,
    }
}

#[test]
fn cargo_nextest_is_test_check() {
    assert_eq!(
        check_kind("scripts/cx nextest run -p mike --lib x"),
        Some("test")
    );
    assert_eq!(check_kind("cargo nextest run"), Some("test"));
    assert_eq!(
        check_kind("flock -w 9 /tmp/l scripts/cx test"),
        Some("test")
    );
}

#[test]
fn scripts_lint_rust_is_lint_check() {
    assert_eq!(check_kind("scripts/lint-rust"), Some("lint"));
    assert_eq!(
        check_kind("scripts/rule-checks/rust-no-comments.sh --cached"),
        Some("lint")
    );
    assert_eq!(check_kind("cargo clippy --all-targets"), Some("lint"));
}

#[test]
fn pnpm_exec_svelte_check_is_typecheck() {
    assert_eq!(
        check_kind("pnpm exec svelte-check --fail-on-warnings"),
        Some("typecheck")
    );
    assert_eq!(check_kind("cargo check -p mike"), Some("build"));
}

#[test]
fn other_kinds_are_recognised() {
    assert_eq!(
        check_kind("scripts/db-perf-check --tier read"),
        Some("perf")
    );
    assert_eq!(check_kind("mike migrate-check"), Some("migrate"));
    assert_eq!(check_kind("scripts/gate check"), Some("gate"));
}

#[test]
fn plain_ls_is_not_a_check() {
    assert_eq!(check_kind("ls -la"), None);
    assert!(derive_checks(&[usage("git status", result("ok", false))], None).is_empty());
}

#[test]
fn exit_code_parsed_from_first_line() {
    let failing = BashResult {
        content: "Exit code 2\nerror[E0599]".to_string(),
        is_error: true,
    };
    assert_eq!(parse_exit_code(&failing), Some(2));
}

#[test]
fn no_exit_line_and_not_error_is_zero() {
    let ok = BashResult {
        content: "test result: ok".to_string(),
        is_error: false,
    };
    assert_eq!(parse_exit_code(&ok), Some(0));
    let unknown = BashResult {
        content: "boom".to_string(),
        is_error: true,
    };
    assert_eq!(parse_exit_code(&unknown), None);
}

#[test]
fn rejected_tool_use_is_rejected_with_null_exit() {
    let rejected = usage(
        "cargo test",
        result(
            "The user doesn't want to proceed with this tool use. The tool use was rejected",
            true,
        ),
    );
    let checks = derive_checks(&[rejected], Some(at(30)));
    assert_eq!(checks.len(), 1);
    assert!(checks[0].rejected);
    assert_eq!(checks[0].exit_code, None);
    assert!(!checks[0].pending);
}

#[test]
fn missing_result_is_pending() {
    let checks = derive_checks(&[usage("cargo test", None)], Some(at(30)));
    assert!(checks[0].pending);
    assert_eq!(checks[0].exit_code, None);
}

#[test]
fn failing_check_counts_as_failed() {
    let failing = usage("cargo test", result("Exit code 101\nfail", true));
    let passing = usage("scripts/lint-rust", result("rust-lint: PASS", false));
    let checks = derive_checks(&[failing, passing], Some(at(30)));
    assert_eq!(checks.len(), 2);
    assert_eq!(failed_count(&checks), 1);
}

fn background(state: &str, exit: Option<i32>, finished: Option<DateTime<Utc>>) -> BashUse {
    let mut launch = usage(
        "cargo nextest run",
        result("Command running in background", false),
    );
    launch.registry = Some(RegistryRow {
        work_item_id: 7,
        state: state.to_string(),
        exit_code: exit,
        finished_at: finished,
    });
    launch
}

#[test]
fn background_launch_takes_registry_exit_when_final_before_close() {
    let checks = derive_checks(&[background("failed", Some(3), Some(at(10)))], Some(at(30)));
    assert!(checks[0].background);
    assert_eq!(checks[0].work_item_id, Some(7));
    assert_eq!(checks[0].exit_code, Some(3));
    assert!(!checks[0].pending);
}

#[test]
fn background_launch_finished_after_close_stays_pending() {
    let checks = derive_checks(
        &[background("completed", Some(0), Some(at(40)))],
        Some(at(30)),
    );
    assert_eq!(checks[0].exit_code, None);
    assert!(checks[0].pending);
    let running = derive_checks(&[background("running", None, None)], Some(at(30)));
    assert!(running[0].pending);
}
