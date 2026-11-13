use super::*;

#[test]
fn preserves_every_failure_before_long_noise_and_deduplicates_runner_progress() {
    let mut output = String::new();
    for i in 0..75 {
        output.push_str(&format!("FAIL [{i}.01s] (1/75) binary test_{i}\n"));
    }
    output.push_str("FAIL [99.99s] (75/75) binary test_0\n");
    output.push_str(&"ordinary output\n".repeat(100));
    let names = inventory(&output);
    assert_eq!(names.len(), 75);
    assert!(names.contains(&"FAIL binary test_0".to_owned()));
    assert!(names.contains(&"FAIL binary test_74".to_owned()));
}

#[test]
fn recognizes_cargo_nextest_vitest_pytest_and_ansi_timeout() {
    let names = inventory(
        "test path::case ... FAILED\nfailures:\n    path::case\n\n\x1b[31mTIMEOUT [60s] binary timed_out\x1b[0m\n FAIL src/a.test.ts > group > fails\nFAILED tests/test_a.py::test_b - AssertionError\nERROR tests/test_c.py::test_d - fixture\n",
    );
    assert_eq!(
        names,
        vec![
            "FAIL path::case",
            "FAIL src/a.test.ts > group > fails",
            "FAIL tests/test_a.py::test_b",
            "FAIL tests/test_c.py::test_d",
            "TIMEOUT binary timed_out"
        ]
    );
}

#[test]
fn extracts_vitest_json_without_assertion_payload() {
    let output = r#"{"testResults":[{"name":"a.test.ts","assertionResults":[{"status":"failed","fullName":"group case","failureMessages":["huge assertion"]},{"status":"passed","fullName":"green"}]}]}"#;
    assert_eq!(inventory(output), vec!["FAIL a.test.ts > group case"]);
}

#[test]
fn unknown_output_is_explicitly_unparsed_and_never_claims_a_complete_suite() {
    let result = summary(&inventory("compiler crashed"), "/exact/run.log");
    assert!(result.contains("unparsed"));
    assert!(result.contains("/exact/run.log"));
    assert!(!result.contains("compiler crashed"));
    assert!(
        summary(
            &inventory("FAIL a.test.ts > early\nstopped after 60 seconds"),
            "/run.log"
        )
        .contains("unparsed or unrun tests may be missing")
    );
}
