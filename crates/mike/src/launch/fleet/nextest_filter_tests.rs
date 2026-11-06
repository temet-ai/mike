use super::{is_nextest_run, list_command, matched, matched_no_test};

const RULED_EXIT_4: &str =
    "error: no tests to run\n\n[mike ruling] observed exit: 4; expected: 0\n";

const RECEIPT_1458: &str = " Nextest run ID 47102b87 with nextest profile: default\n    Starting 0 tests across 1 binary (6460 tests skipped, including 4 tests via profile.default.default-filter)\n     Summary [   0.010s] 0 tests run: 0 passed, 6460 skipped\nerror: no tests to run\n(hint: use `--no-tests` to customize)";

#[test]
fn a_nextest_run_that_ran_no_test_is_recognised_bare_or_ruled() {
    let command = "scripts/cx nextest run -p mike --lib launch::fleet::events";
    assert!(matched_no_test(command, Some(4), RECEIPT_1458));
    assert!(matched_no_test(command, Some(1), RULED_EXIT_4));
    assert!(!matched_no_test(command, Some(4), "test a ... FAILED"));
    assert!(!matched_no_test(
        command,
        Some(1),
        "[mike ruling] observed exit: 100; expected: 0"
    ));
    assert!(!matched_no_test(
        "cargo test -p mike",
        Some(4),
        RECEIPT_1458
    ));
}

#[test]
fn only_nextest_run_counts_as_a_nextest_run() {
    assert!(is_nextest_run("cargo nextest run -p mike"));
    assert!(!is_nextest_run("cargo nextest list -p mike"));
    assert!(!is_nextest_run("scripts/gate check"));
}

#[test]
fn the_list_form_keeps_the_filter_and_profile_and_asks_for_json() {
    assert_eq!(
        list_command("scripts/cx nextest run -p mike --lib store::x").as_deref(),
        Some("scripts/cx nextest list -p mike --lib store::x --message-format json")
    );
    assert_eq!(
        list_command("cargo nextest run --profile ci -p mike -- --exact a::b").as_deref(),
        Some("cargo nextest list --profile ci -p mike --message-format json -- --exact a::b")
    );
    assert_eq!(list_command("cargo nextest run -p mike | tail"), None);
    assert_eq!(list_command("cargo test -p mike"), None);
}

#[test]
fn a_listing_counts_only_the_tests_the_filter_and_profile_match() {
    let none = r#"{"test-count":2,"rust-suites":{"mike::lib":{"testcases":{
        "store::a":{"filter-match":{"status":"mismatch","reason":"default-filter"}},
        "store::b":{"filter-match":{"status":"mismatch","reason":"string"}}}}}}"#;
    assert_eq!(matched(none), Some(0));
    let one = r#"{"rust-suites":{"mike::lib":{"testcases":{
        "store::a":{"filter-match":{"status":"matches"}}}}}}"#;
    assert_eq!(matched(one), Some(1));
    assert_eq!(matched("Compiling mike"), None);
}
