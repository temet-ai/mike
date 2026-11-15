use serde_json::{Value, json};

use super::promotion::PromotionState;
use super::render_activity::{render_memory_grammar, render_proposer_days, render_skill_live};
use super::render_quality::{render_lesson_quality, render_placement};

fn joined(lines: Vec<String>) -> String {
    lines.join("\n")
}

fn ledger(keys: &[(&str, Value)]) -> PromotionState {
    PromotionState {
        ledger: keys
            .iter()
            .map(|(key, entry)| ((*key).to_owned(), entry.clone()))
            .collect(),
        manifest_names: Default::default(),
    }
}

#[test]
fn proposer_days_render_one_line_per_month() {
    let data = json!({"proposer_active_days": [
        {"month": "2026-09", "days_with_calls": 4},
        {"month": "2026-10", "days_with_calls": 2},
    ]});
    assert_eq!(
        render_proposer_days(&data),
        [
            "- Days with at least one lesson proposer call, UTC month 2026-09: 4.",
            "- Days with at least one lesson proposer call, UTC month 2026-10: 2.",
        ]
    );
    assert!(
        joined(render_proposer_days(&json!({"proposer_active_days": []})))
            .contains("No lesson proposer call")
    );
    assert!(render_proposer_days(&json!({})).is_empty());
}

#[test]
fn memory_grammar_renders_totals_and_a_row_per_project() {
    let data = json!({
        "memory_recall_tags": [{"project": "p", "no_tag_requests": 5, "injected": 2,
                               "delivered_out_of_band": 1, "requests_with_recall_tags": 4}],
        "memory_update_tags": [{"project": "p", "replies_with_tags": 2, "tags_found": 3,
                                "lessons_written": 2, "lessons_skipped": 1, "lessons_truncated": 0}],
        "memory_harvested_lessons": [{"project": "p", "lessons": 2}],
    });
    let text = joined(render_memory_grammar(&data));
    assert!(text.contains("4 requests carried one; 3 led to lessons being delivered"));
    assert!(text.contains("3 found in replies; 2 stored as lessons and 1 skipped"));
    assert!(text.contains("| p | 4 | 3 | 3 | 2 | 1 |"));
}

#[test]
fn memory_grammar_is_absent_without_its_queries() {
    assert!(render_memory_grammar(&json!({})).is_empty());
}

#[test]
fn skill_live_renders_both_rates_and_the_retirement_reason() {
    let shipped = json!({"project": "/p/app", "folder": "deploy", "first_shipped_on": "2026-10-14",
                         "loads": 12, "judged_turns": 8, "corrected_turns": 4,
                         "base_judged_turns": 40, "base_corrected_turns": 8,
                         "retired_at": null, "retired_reason": null});
    let gone = json!({"project": "/p/app", "folder": "lint", "first_shipped_on": "2026-10-14",
                      "loads": 0, "judged_turns": 0, "corrected_turns": 0,
                      "base_judged_turns": 40, "base_corrected_turns": 8,
                      "retired_at": "2026-10-20T00:00:00+00:00",
                      "retired_reason": "corrected_above_base"});
    let text = joined(render_skill_live(&json!({"skill_live": [shipped, gone]})));
    assert!(text.contains(
        "| /p/app | `deploy` | 2026-10-14 | 12 | 4 of 8 (50%) | 8 of 40 (20%) | shipped |"
    ));
    assert!(text.contains(
        "| /p/app | `lint` | 2026-10-14 | 0 | no judged turn | 8 of 40 (20%) | retired (corrected_above_base) |"
    ));
}

#[test]
fn skill_live_says_so_when_the_table_is_missing_or_empty() {
    assert!(joined(render_skill_live(&json!({"skill_live": null}))).contains("Not available"));
    assert!(joined(render_skill_live(&json!({}))).contains("Not available"));
    assert!(joined(render_skill_live(&json!({"skill_live": []}))).contains("No shipped skill"));
}

fn quality_data() -> Value {
    json!({
        "lesson_shape": [
            {"shape": "over_length", "lessons": 2, "stored_chars": 2400, "over_600_chars": 2, "under_80_chars": 0},
            {"shape": "rule", "lessons": 8, "stored_chars": 1200, "over_600_chars": 0, "under_80_chars": 1},
        ],
        "lesson_shape_injected": [
            {"shape": "over_length", "injections": 10, "injected_chars": 24000},
            {"shape": "rule", "injections": 30, "injected_chars": 6000},
        ],
        "lesson_shape_pending": [{"shape": "over_length", "lessons": 2, "oldest_found_at": "2026-10-14"}],
        "lesson_shape_resolved": [{"resolution": "rewritten", "lessons": 3}],
        "lesson_relations_write_time": [
            {"kind": "supersedes", "method": "write_time", "status": "confirmed", "relations": 2, "applied": 1}
        ],
        "lesson_replacement_effect": [
            {"side": "replaced", "turns": 40, "corrected_turns": 4},
            {"side": "replacement", "turns": 25, "corrected_turns": 1},
        ],
    })
}

#[test]
fn lesson_quality_shows_counts_by_shape_and_the_share_of_injected_characters() {
    let text = joined(render_lesson_quality(&quality_data()));
    for expected in [
        "Active lessons: 10; mean length 360 characters; 2 over 600 characters; 1 under 80 characters.",
        "| over_length | 2 | 2400 | 10 | 24000 | 80.0% |",
        "| rule | 8 | 1200 | 30 | 6000 | 20.0% |",
        "Waiting for a rewrite: 2 `over_length` lessons",
        "Resolved this window: 3 rewritten.",
        "`write_time` `supersedes` `confirmed`: 2 (1 applied by consolidation).",
        "Turns that received the replaced lesson: 40, 4 corrected (10.0%).",
        "Turns that received the replacement lesson: 25, 1 corrected (4.0%).",
    ] {
        assert!(text.contains(expected), "missing {expected:?} in\n{text}");
    }
}

#[test]
fn lesson_quality_says_so_when_the_migration_has_not_run() {
    let text = joined(render_lesson_quality(&json!({"lesson_shape": null})));
    assert!(text.contains("Not available"));
    assert!(render_lesson_quality(&json!({})).is_empty());
}

fn placement_data() -> Value {
    json!({
        "lesson_placement": [
            {"placement": "memory", "lessons": 20, "agent_scoped": 3, "naming_a_document": 1},
            {"placement": "rule", "lessons": 2, "agent_scoped": 0, "naming_a_document": 0},
            {"placement": "skill", "lessons": 8, "agent_scoped": 1, "naming_a_document": 2},
            {"placement": "unplaced", "lessons": 5, "agent_scoped": 0, "naming_a_document": 0},
        ],
        "lesson_placement_stuck": [{"lessons": 1}],
        "lesson_placement_proposals": [
            {"id": 41, "placement": "rule", "project": "/p/mike", "agent": null,
             "text": "Number every migration above the newest one on main.",
             "covered_by": null, "reason": "It applies to migration files"}
        ],
        "project_names": [
            {"project_uuid": "u1", "project_path": "/p/mike"},
            {"project_uuid": "u2", "project_path": "/p/other"},
        ],
        "skill_cap_setting": [{"value": 2}],
    })
}

fn placement_ledger() -> PromotionState {
    ledger(&[
        ("u1:alpha", json!({"retired_at": null})),
        ("u1:bravo", json!({"retired_at": null})),
        ("u1:charlie", json!({"retired_at": null})),
        ("u1:old", json!({"retired_at": "2026-10-01"})),
        ("u2:delta", json!({"retired_at": null})),
    ])
}

#[test]
fn placement_counts_lessons_and_lists_what_could_not_be_placed() {
    let text = joined(render_placement(&placement_data(), &placement_ledger()));
    for expected in [
        "| memory | 20 | 3 | 1 |",
        "| skill | 8 | 1 | 2 |",
        "| unplaced | 5 | 0 | 0 |",
        "Active lessons: 35; 1 could not be placed after three attempts.",
    ] {
        assert!(text.contains(expected), "missing {expected:?} in\n{text}");
    }
}

#[test]
fn placement_shows_skills_per_project_against_the_cap() {
    let text = joined(render_placement(&placement_data(), &placement_ledger()));
    assert!(text.contains("cap of 2 per project"));
    assert!(text.contains("| /p/mike | 3 | 2 | over the cap |"));
    assert!(text.contains("| /p/other | 1 | 2 | within the cap |"));
}

#[test]
fn placement_lists_each_rule_proposal_in_full() {
    let text = joined(render_placement(&placement_data(), &placement_ledger()));
    assert!(text.contains("### Proposed rule entry: lesson 41 (/p/mike)"));
    assert!(text.contains("Number every migration above the newest one on main."));
    assert!(text.contains("Why: It applies to migration files."));
}

#[test]
fn placement_says_so_when_the_migration_has_not_run() {
    let state = placement_ledger();
    assert!(
        joined(render_placement(&json!({"lesson_placement": null}), &state))
            .contains("Not available")
    );
    assert!(render_placement(&json!({}), &state).is_empty());
}

#[test]
fn placement_uses_the_default_cap_when_no_setting_is_stored() {
    let mut data = placement_data();
    data["skill_cap_setting"] = json!([]);
    let text = joined(render_placement(&data, &placement_ledger()));
    assert!(text.contains("cap of 12 per project"));
    assert!(text.contains("| /p/mike | 3 | 12 | within the cap |"));
}
