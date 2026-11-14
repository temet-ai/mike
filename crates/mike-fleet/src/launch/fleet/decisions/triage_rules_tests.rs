use super::*;

#[test]
fn by_rules_a_killed_command_is_an_environment_problem() {
    let found = by_rules(None, "running tests");
    assert_eq!(found.map(|t| t.action), Some(Action::FixEnvironment));
}

#[test]
fn by_rules_a_timeout_exit_code_is_an_environment_problem() {
    let found = by_rules(Some(124), "partial output");
    assert_eq!(found.map(|t| t.action), Some(Action::FixEnvironment));
}

#[test]
fn by_rules_a_timeout_message_in_the_output_is_an_environment_problem() {
    let found = by_rules(Some(1), "error: the check timed out after 900 seconds");
    assert_eq!(found.map(|t| t.action), Some(Action::FixEnvironment));
}

#[test]
fn by_rules_a_missing_python_module_names_the_module() {
    let found = by_rules(
        Some(1),
        "E   ModuleNotFoundError: No module named 'requests'",
    );
    let Some(found) = found else {
        panic!("a missing module is decided by rule");
    };
    assert_eq!(found.action, Action::FixEnvironment);
    assert!(found.reason.contains("requests"));
}

#[test]
fn by_rules_a_missing_command_is_an_environment_problem() {
    let found = by_rules(Some(127), "sh: 1: cargo-nextest: command not found");
    assert_eq!(found.map(|t| t.action), Some(Action::FixEnvironment));
}

#[test]
fn by_rules_a_rust_compile_error_names_the_file() {
    let output = "error[E0425]: cannot find value `x` in this scope\n  --> src/lib.rs:12:5\n";
    let Some(found) = by_rules(Some(101), output) else {
        panic!("a compile error is decided by rule");
    };
    assert_eq!(found.action, Action::FixCode);
    assert!(found.reason.contains("src/lib.rs:12"));
}

#[test]
fn by_rules_a_typescript_error_names_the_file() {
    let output = "src/app.ts(7,3): error TS2304: Cannot find name 'foo'.";
    let Some(found) = by_rules(Some(2), output) else {
        panic!("a type error is decided by rule");
    };
    assert_eq!(found.action, Action::FixCode);
    assert!(found.reason.contains("src/app.ts:7"));
}

#[test]
fn by_rules_a_plain_test_failure_is_left_to_the_model() {
    assert_eq!(by_rules(Some(1), "assertion `left == right` failed"), None);
}

#[test]
fn triage_line_says_who_decided_and_what_to_do() {
    let line = Triage {
        action: Action::Retry,
        reason: "flaky".to_owned(),
        by: "jev",
    }
    .line();
    assert_eq!(line, "triage (jev): retry, it looks transient; flaky");
}

#[test]
fn action_keys_round_trip() {
    for action in [Action::Retry, Action::FixCode, Action::FixEnvironment] {
        assert_eq!(Action::from_key(action.key()), Some(action));
    }
    assert_eq!(Action::from_key("other"), None);
}
