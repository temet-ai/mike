use serde_json::{Value, json};

use super::findings::{
    Finding, findings, generated_skill_loads, live_findings, memory_grammar_totals, opportunity,
};
use super::promotion::PromotionState;

fn promotion(ledger: &[&str], manifest: &[&str]) -> PromotionState {
    PromotionState {
        ledger: ledger
            .iter()
            .map(|key| ((*key).to_owned(), json!({})))
            .collect(),
        manifest_names: manifest.iter().map(|name| (*name).to_owned()).collect(),
    }
}

fn kinds(found: &[Finding]) -> Vec<&str> {
    found.iter().map(|finding| finding.kind.as_str()).collect()
}

fn grammar_data(recall: Value, update: Value, harvested: Value) -> Value {
    json!({
        "embedding_setting": [], "injections": [], "lessons": [], "turns": [],
        "effects": [{"family": "action_effect", "rows": 1}],
        "memory_recall_tags": recall,
        "memory_update_tags": update,
        "memory_harvested_lessons": harvested,
    })
}

fn update_row(found: i64, written: i64, skipped: i64) -> Value {
    json!({"project": "p", "replies_with_tags": 1, "tags_found": found,
           "lessons_written": written, "lessons_skipped": skipped, "lessons_truncated": 0})
}

fn skill_data(loads: Value) -> Value {
    json!({
        "embedding_setting": [], "injections": [], "lessons": [], "turns": [],
        "effects": [{"family": "action_effect", "rows": 1}],
        "skill_loads": loads,
    })
}

#[test]
fn findings_machine_only_activity_reports_only_incomplete_provenance() {
    let data = json!({
        "embedding_setting": [], "injections": [], "lessons": [],
        "effects": [{"family": "action_effect", "rows": 1}],
        "turns": [
            {"project": "machine", "turns": 20, "genuine_turns": 0},
            {"project": "unknown", "turns": 3, "genuine_turns": 0, "missing_provenance": 3},
        ],
    });
    let found = findings(&data, &PromotionState::default());
    assert_eq!(kinds(&found), ["outcome_provenance_incomplete"]);
}

#[test]
fn findings_disabled_embedding_and_missing_delivery_are_four_findings() {
    let data = json!({
        "embedding_setting": [{"value": false}], "injections": [],
        "turns": [{"project": "p", "turns": 4}],
        "lessons": [{"project": "p", "source": "operator", "pending_embeddings": 2}],
        "effects": [],
    });
    let found = findings(&data, &PromotionState::default());
    assert_eq!(
        kinds(&found),
        [
            "embedding_disabled",
            "no_recorded_lesson_delivery",
            "operator_lessons_unindexed",
            "effect_measurement_missing"
        ]
    );
}

#[test]
fn opportunity_marks_a_candidate_as_requiring_validation() {
    let candidate = opportunity(&json!({"example_request_id": 42}));
    assert_eq!(candidate["status"], "candidate_requires_validation");
    assert!(
        candidate["savings_status"]
            .as_str()
            .is_some_and(|text| text.contains("not proven"))
    );
    assert_eq!(
        candidate["evidence_url"],
        "http://localhost:9989/api/requests/42/tool-calls"
    );
}

#[test]
fn findings_worker_toggle_off_for_days_is_reported_once() {
    let mut data = grammar_data(json!([]), json!([update_row(1, 1, 0)]), json!([]));
    data["worker_toggles"] = json!([
        {"key": "judge_lessons", "enabled": false, "days_in_state": "9.2"},
        {"key": "judge_worker", "enabled": false, "days_in_state": "0.5"},
        {"key": "steerer", "enabled": true, "days_in_state": "30.0"},
    ]);
    data["llm_roles"] = json!([]);
    let found = findings(&data, &PromotionState::default());
    assert_eq!(kinds(&found), ["worker_toggle_off"]);
    assert_eq!(
        found[0].evidence.as_ref().map(|row| row["key"].clone()),
        Some(json!("judge_lessons"))
    );
}

#[test]
fn findings_llm_role_whose_last_call_failed_is_reported() {
    let mut data = grammar_data(json!([]), json!([update_row(1, 1, 0)]), json!([]));
    data["worker_toggles"] = json!([]);
    data["llm_roles"] = json!([
        {"role": "judge_worker", "last_outcome": "http_error", "failed_streak": 5},
        {"role": "steerer", "last_outcome": "ok", "failed_streak": 0},
    ]);
    let found = findings(&data, &PromotionState::default());
    assert_eq!(kinds(&found), ["llm_role_failing"]);
    assert_eq!(
        found[0].evidence.as_ref().map(|row| row["role"].clone()),
        Some(json!("judge_worker"))
    );
}

#[test]
fn findings_memory_grammar_unused_fires_when_no_tag_was_seen() {
    let data = grammar_data(json!([]), json!([]), json!([]));
    assert_eq!(
        kinds(&findings(&data, &PromotionState::default())),
        ["memory_grammar_unused"]
    );
}

#[test]
fn findings_memory_grammar_is_quiet_when_update_tags_were_found() {
    let data = grammar_data(json!([]), json!([update_row(2, 1, 1)]), json!([]));
    assert!(findings(&data, &PromotionState::default()).is_empty());
}

#[test]
fn findings_memory_grammar_is_quiet_when_recall_tags_were_seen() {
    let recall = json!([{"project": "p", "no_tag_requests": 9, "injected": 1,
                         "delivered_out_of_band": 0, "requests_with_recall_tags": 1}]);
    let data = grammar_data(recall, json!([]), json!([]));
    assert!(findings(&data, &PromotionState::default()).is_empty());
}

#[test]
fn memory_grammar_totals_sum_every_project() {
    let data = grammar_data(
        json!([{"project": "p", "no_tag_requests": 5, "injected": 2,
                "delivered_out_of_band": 1, "requests_with_recall_tags": 4}]),
        json!([update_row(3, 2, 1)]),
        json!([{"project": "p", "lessons": 2}]),
    );
    let totals = memory_grammar_totals(&data);
    assert_eq!(
        (
            totals.recall_tag_requests,
            totals.recall_injected,
            totals.update_tags_found,
            totals.update_lessons_written,
            totals.update_lessons_skipped,
            totals.harvested_lessons,
        ),
        (4, 3, 3, 2, 1, 2)
    );
}

#[test]
fn findings_skill_loads_without_a_generated_skill_are_reported() {
    let data = skill_data(json!([
        {"project": "p", "project_uuid": "u1", "skill": "manual-skill", "loads": 5, "sessions": 2}
    ]));
    let found = findings(&data, &PromotionState::default());
    assert_eq!(
        found,
        [Finding::new(
            "generated_skills_never_loaded",
            "global",
            Some(json!({"total_loads": 5, "generated_loads": 0}))
        )]
    );
}

#[test]
fn findings_a_generated_skill_load_suppresses_the_skill_finding() {
    let data = skill_data(json!([
        {"project": "p", "project_uuid": "u1", "skill": "manual-skill", "loads": 5, "sessions": 2},
        {"project": "p", "project_uuid": "u1", "skill": "mike-memory", "loads": 1, "sessions": 1},
    ]));
    let found = findings(&data, &promotion(&[], &["mike-memory"]));
    assert!(!kinds(&found).contains(&"generated_skills_never_loaded"));
}

#[test]
fn generated_skill_loads_match_through_the_manifest_whatever_the_name() {
    let data = skill_data(json!([
        {"project": "p", "project_uuid": "u1", "skill": "lookalike", "loads": 2, "sessions": 1},
        {"project": "p", "project_uuid": null, "skill": "rust-lint", "loads": 1, "sessions": 1},
    ]));
    let flags = |state: &PromotionState| -> Vec<bool> {
        generated_skill_loads(&data, state)
            .iter()
            .map(|load| load.generated)
            .collect()
    };
    assert_eq!(
        flags(&promotion(&["u1:rust-lint"], &["shared-workflows"])),
        [false, true]
    );
    assert_eq!(flags(&promotion(&[], &["lookalike"])), [true, false]);
}

#[test]
fn findings_a_topic_skill_load_counts_as_generated_through_the_ledger() {
    let data = skill_data(json!([
        {"project": "p", "project_uuid": "u1", "skill": "deploy-and-restart", "loads": 3, "sessions": 1}
    ]));
    let found = findings(&data, &promotion(&["u1:deploy-and-restart"], &[]));
    assert!(!kinds(&found).contains(&"generated_skills_never_loaded"));
}

#[test]
fn live_findings_flag_only_skills_retired_for_a_corrected_rate() {
    let shipped = json!({"project": "/p/app", "folder": "deploy", "retired_at": null, "retired_reason": null});
    let retired = json!({"project": "/p/app", "folder": "lint",
                         "retired_at": "2026-10-20T00:00:00+00:00",
                         "retired_reason": "corrected_above_base"});
    let found = live_findings(&json!({"skill_live": [shipped, retired]}));
    assert_eq!(
        found
            .iter()
            .map(|finding| finding.evidence.as_ref().map(|row| row["folder"].clone()))
            .collect::<Vec<_>>(),
        [Some(json!("lint"))]
    );
    assert!(live_findings(&json!({"skill_live": null})).is_empty());
}
