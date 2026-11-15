use serde_json::{Value, json};

use super::canonical::canonical_json;
use super::qualification::{policy_hash, qualifies};
use super::test_support::at;

fn run() -> Value {
    json!({
        "execution_kind": "actual",
        "execution_ref": "fixture-execution",
        "correct": true,
        "verdict_ref": "fixture-grader",
        "started_at": "2026-10-18T10:00:00Z",
        "finished_at": "2026-10-18T10:01:00Z",
        "calls": [{"id": "read", "content": "x"}],
        "model_turns": [{"id": "turn", "input_tokens": 1, "output_tokens": 1}],
        "task_hash": "task",
        "ground_truth_hash": "gold",
        "artifact_hash": "tree",
        "model_config_hash": "model",
        "activation_ref": "fixture-activation",
        "constraint_verdict_ref": "fixture-constraints"
    })
}

fn evidence(policy: &Value) -> Value {
    let pairs: Vec<Value> = ["documentation", "configuration", "test-output"]
        .into_iter()
        .flat_map(|family| {
            (0..3).map(move |i| {
                json!({
                    "task_id": format!("{family}-{i}"),
                    "family": family,
                    "original": run(),
                    "compact": run()
                })
            })
        })
        .collect();
    json!({
        "schema": "compressor-qualification-v1",
        "policy_sha256": policy_hash(policy),
        "completed_at": "2026-10-18T11:00:00Z",
        "recovery": {
            "passed": true,
            "weakened_control_failed": true,
            "evidence_ref": "fixture-recovery"
        },
        "pairs": pairs
    })
}

fn now() -> chrono::DateTime<chrono::Utc> {
    at("2026-10-18T12:00:00Z")
}

#[test]
fn qualifies_complete_current_matching_evidence() {
    let policy = json!({"version": 3});
    assert!(qualifies(Some(&evidence(&policy)), &policy, now()));
}

#[test]
fn qualifies_rejects_a_different_policy_or_missing_evidence() {
    let policy = json!({"version": 3});
    let found = evidence(&policy);
    assert!(!qualifies(Some(&found), &json!({"version": 4}), now()));
    assert!(!qualifies(None, &policy, now()));
    assert!(!qualifies(Some(&json!("text")), &policy, now()));
}

#[test]
fn qualifies_rejects_stale_or_future_evidence() {
    let policy = json!({"version": 3});
    let found = evidence(&policy);
    assert!(!qualifies(
        Some(&found),
        &policy,
        at("2026-10-26T00:00:00Z")
    ));
    assert!(!qualifies(
        Some(&found),
        &policy,
        at("2026-10-18T10:00:00Z")
    ));
}

#[test]
fn qualifies_rejects_a_failed_or_unevidenced_trial() {
    let policy = json!({"version": 3});
    for field in ["correct", "activation_ref", "constraint_verdict_ref"] {
        let mut bad = evidence(&policy);
        bad["pairs"][0]["compact"][field] = json!(false);
        assert!(!qualifies(Some(&bad), &policy, now()), "{field}");
    }
}

#[test]
fn qualifies_rejects_failed_recovery() {
    let policy = json!({"version": 3});
    let mut bad = evidence(&policy);
    bad["recovery"]["passed"] = json!(false);
    assert!(!qualifies(Some(&bad), &policy, now()));
}

#[test]
fn qualifies_rejects_too_few_tasks_in_a_family() {
    let policy = json!({"version": 3});
    let mut bad = evidence(&policy);
    if let Some(pairs) = bad["pairs"].as_array_mut() {
        pairs.truncate(8);
    }
    assert!(!qualifies(Some(&bad), &policy, now()));
}

#[test]
fn qualifies_rejects_a_pair_whose_arms_differ_in_task() {
    let policy = json!({"version": 3});
    let mut bad = evidence(&policy);
    bad["pairs"][0]["compact"]["task_hash"] = json!("other");
    assert!(!qualifies(Some(&bad), &policy, now()));
}

#[test]
fn qualifies_rejects_a_policy_with_an_implementation_but_no_build() {
    let policy = json!({"implementation": "v3", "build_sha": null});
    assert!(!qualifies(Some(&evidence(&policy)), &policy, now()));
    let built = json!({"implementation": "v3", "build_sha": "abc"});
    assert!(qualifies(Some(&evidence(&built)), &built, now()));
}

#[test]
fn policy_hash_is_the_sha256_of_the_sorted_compact_json() {
    assert_eq!(
        policy_hash(&json!({"version": 3})),
        "9af23cea10a478f3bb916ff35c07a36debe444adf9b56814ab63861ec674ab8b"
    );
}

#[test]
fn canonical_json_sorts_keys_and_escapes_non_ascii() {
    let value = json!({"b": [1, "\u{e9}", "a\"b"], "a": null});
    assert_eq!(
        canonical_json(&value),
        "{\"a\":null,\"b\":[1,\"\\u00e9\",\"a\\\"b\"]}"
    );
    assert_eq!(canonical_json(&json!("\u{1f600}")), "\"\\ud83d\\ude00\"");
}
