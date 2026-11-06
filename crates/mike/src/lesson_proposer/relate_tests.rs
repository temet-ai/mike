use chrono::{TimeZone, Utc};
use serde_json::json;

use super::*;

fn neighbour(id: i64, source: &str) -> NeighbourLesson {
    NeighbourLesson {
        id,
        text: format!("existing lesson {id}"),
        distance: 0.1,
        source: source.to_owned(),
        created_at: Utc
            .with_ymd_and_hms(2026, 9, 1, 10, 0, 0)
            .single()
            .unwrap_or_default(),
        source_turn_id: Some(id * 10),
        session_id: Some(id * 100),
    }
}

fn context() -> NewLessonContext {
    NewLessonContext {
        verdict_id: 77,
        source_turn_id: 5,
        max_chars: 500,
    }
}

fn memory_placement() -> PlacementAnswer {
    PlacementAnswer {
        home: Home::Memory,
        covered_by: String::new(),
        reason: "a fact".to_owned(),
        persona: false,
    }
}

fn answer(decisions: &[(i64, Relation)], replacement: &str) -> RelateAnswer {
    RelateAnswer {
        decisions: decisions
            .iter()
            .map(|(id, decision)| CandidateDecision {
                lesson_id: *id,
                decision: *decision,
                reason: format!("reason for {id}"),
            })
            .collect(),
        replacement: replacement.to_owned(),
        placement: Some(memory_placement()),
    }
}

fn skill_row() -> GuidanceRow {
    GuidanceRow {
        reference: "artifact:9".to_owned(),
        source_name: "skills".to_owned(),
        relative_path: "deploy-the-proxy/SKILL.md".to_owned(),
        title: "deploy-the-proxy".to_owned(),
        body: "Run scripts/start-proxy.sh.".to_owned(),
        metadata: json!({"name": "deploy-the-proxy"}),
        similarity: Some(0.8),
    }
}

fn run(answer: &RelateAnswer, neighbours: &[NeighbourLesson]) -> WriteTimeVerdict {
    resolve(answer, neighbours, &[], &context())
}

const GOOD: &str =
    "Run scripts/gate lint before committing; a green cargo check does not prove the limits hold.";

#[test]
fn all_store_decisions_write_nothing_extra() {
    let neighbours = vec![neighbour(1, "judge"), neighbour(2, "judge")];
    let verdict = run(
        &answer(&[(1, Relation::Store), (2, Relation::Store)], ""),
        &neighbours,
    );
    assert_eq!(verdict.action, WriteAction::Store);
    assert!(verdict.relations.is_empty());
    assert!(verdict.notes.is_empty());
    assert_eq!(verdict.placement.home, Home::Memory);
}

#[test]
fn skip_targets_the_lesson_and_records_a_recurs_relation() {
    let neighbours = vec![neighbour(1, "judge")];
    let verdict = run(&answer(&[(1, Relation::Skip)], ""), &neighbours);
    assert_eq!(verdict.action, WriteAction::Skip { target: 1 });
    assert_eq!(verdict.relations.len(), 1);
    assert_eq!(verdict.relations[0].kind, "recurs");
    assert_eq!(verdict.relations[0].to_lesson_id, 1);
    assert_eq!(verdict.relations[0].evidence["verdict_id"], 77);
    assert_eq!(
        verdict.relations[0].evidence["related"]["source_turn_id"],
        10
    );
}

#[test]
fn skip_wins_over_update_and_drops_the_other_relations() {
    let neighbours = vec![neighbour(1, "judge"), neighbour(2, "judge")];
    let verdict = run(
        &answer(&[(1, Relation::Update), (2, Relation::Skip)], GOOD),
        &neighbours,
    );
    assert_eq!(verdict.action, WriteAction::Skip { target: 2 });
    assert_eq!(verdict.relations.len(), 1);
}

#[test]
fn update_replaces_with_a_supersedes_relation() {
    let neighbours = vec![neighbour(1, "judge")];
    let verdict = run(&answer(&[(1, Relation::Update)], GOOD), &neighbours);
    assert_eq!(
        verdict.action,
        WriteAction::Replace {
            replacement: GOOD.to_owned()
        }
    );
    assert_eq!(verdict.relations[0].kind, "supersedes");
    assert_eq!(verdict.replacement(), Some(GOOD));
}

#[test]
fn merge_of_two_lessons_records_a_refines_relation_for_each() {
    let neighbours = vec![neighbour(1, "judge"), neighbour(2, "operator")];
    let verdict = run(
        &answer(&[(1, Relation::Merge), (2, Relation::Merge)], GOOD),
        &neighbours,
    );
    assert!(matches!(verdict.action, WriteAction::Replace { .. }));
    let kinds: Vec<&str> = verdict.relations.iter().map(|r| r.kind.as_str()).collect();
    assert_eq!(kinds, vec!["refines", "refines"]);
}

#[test]
fn update_without_a_usable_replacement_degrades_to_store_with_a_note() {
    let neighbours = vec![neighbour(1, "judge")];
    let verdict = run(&answer(&[(1, Relation::Update)], "  "), &neighbours);
    assert_eq!(verdict.action, WriteAction::Store);
    assert!(verdict.relations.is_empty());
    assert!(
        verdict
            .notes
            .iter()
            .any(|n| n.contains("without a replacement"))
    );
}

#[test]
fn an_over_length_replacement_is_rejected_and_the_note_names_the_limit() {
    let neighbours = vec![neighbour(1, "judge")];
    let long = "Verify every claim against the running system. ".repeat(20);
    let verdict = run(&answer(&[(1, Relation::Merge)], &long), &neighbours);
    assert_eq!(verdict.action, WriteAction::Store);
    assert!(
        verdict.notes.iter().any(|n| n.contains("limit is 500")),
        "{:?}",
        verdict.notes
    );
}

#[test]
fn contradicts_is_recorded_beside_a_plain_store() {
    let neighbours = vec![neighbour(1, "judge")];
    let verdict = run(&answer(&[(1, Relation::Contradicts)], ""), &neighbours);
    assert_eq!(verdict.action, WriteAction::Store);
    assert_eq!(verdict.relations.len(), 1);
    assert_eq!(verdict.relations[0].kind, "contradicts");
}

#[test]
fn contradicts_survives_next_to_an_update() {
    let neighbours = vec![neighbour(1, "judge"), neighbour(2, "judge")];
    let verdict = run(
        &answer(&[(1, Relation::Update), (2, Relation::Contradicts)], GOOD),
        &neighbours,
    );
    let kinds: Vec<&str> = verdict.relations.iter().map(|r| r.kind.as_str()).collect();
    assert_eq!(kinds, vec!["supersedes", "contradicts"]);
}

#[test]
fn a_decision_for_a_lesson_that_was_not_shown_is_ignored_and_noted() {
    let neighbours = vec![neighbour(1, "judge")];
    let verdict = run(&answer(&[(99, Relation::Skip)], ""), &neighbours);
    assert_eq!(verdict.action, WriteAction::Store);
    assert!(verdict.notes.iter().any(|n| n.contains("lesson 99")));
}

#[test]
fn a_second_decision_for_the_same_lesson_is_ignored() {
    let neighbours = vec![neighbour(1, "judge")];
    let verdict = run(
        &answer(&[(1, Relation::Store), (1, Relation::Skip)], ""),
        &neighbours,
    );
    assert_eq!(verdict.action, WriteAction::Store);
    assert!(verdict.notes.iter().any(|n| n.contains("second decision")));
}

#[test]
fn no_neighbours_still_yields_a_verdict_with_the_placement() {
    let mut given = answer(&[], "");
    given.placement = Some(PlacementAnswer {
        home: Home::Skill,
        covered_by: "deploy-the-proxy".to_owned(),
        reason: "a procedure".to_owned(),
        persona: false,
    });
    let verdict = resolve(&given, &[], &[skill_row()], &context());
    assert_eq!(verdict.action, WriteAction::Store);
    assert_eq!(verdict.placement.home, Home::Skill);
    assert_eq!(
        verdict.placement.covered_by.as_deref(),
        Some("deploy-the-proxy/SKILL.md")
    );
}

#[test]
fn nowhere_with_a_shown_document_discards_the_lesson() {
    let mut given = answer(&[], "");
    given.placement = Some(PlacementAnswer {
        home: Home::Nowhere,
        covered_by: "deploy-the-proxy".to_owned(),
        reason: "the skill says it".to_owned(),
        persona: false,
    });
    let verdict = resolve(&given, &[], &[skill_row()], &context());
    assert_eq!(
        verdict.action,
        WriteAction::Discard {
            covered_by: "deploy-the-proxy/SKILL.md".to_owned()
        }
    );
}

#[test]
fn nowhere_beside_a_contradiction_is_stored_as_memory() {
    let neighbours = vec![neighbour(1, "judge")];
    let mut given = answer(&[(1, Relation::Contradicts)], "");
    given.placement = Some(PlacementAnswer {
        home: Home::Nowhere,
        covered_by: "deploy-the-proxy".to_owned(),
        reason: "the skill says it".to_owned(),
        persona: false,
    });
    let verdict = resolve(&given, &neighbours, &[skill_row()], &context());
    assert_eq!(verdict.action, WriteAction::Store);
    assert_eq!(verdict.placement.home, Home::Memory);
}

#[test]
fn nowhere_beside_a_replacement_is_stored_as_memory() {
    let neighbours = vec![neighbour(1, "judge")];
    let mut given = answer(&[(1, Relation::Update)], GOOD);
    given.placement = Some(PlacementAnswer {
        home: Home::Nowhere,
        covered_by: "deploy-the-proxy".to_owned(),
        reason: "the skill says it".to_owned(),
        persona: false,
    });
    let verdict = resolve(&given, &neighbours, &[skill_row()], &context());
    assert!(matches!(verdict.action, WriteAction::Replace { .. }));
    assert_eq!(verdict.placement.home, Home::Memory);
}

#[test]
fn input_shows_every_lesson_in_full_with_id_source_and_limit() {
    let mut long = neighbour(3, "operator");
    long.text = "word ".repeat(400);
    let input = build_input("New rule text", std::slice::from_ref(&long), &[], 500);
    assert!(input.contains("## CHARACTER LIMIT FOR replacement\n500"));
    assert!(input.contains("New rule text"));
    assert!(input.contains("lesson_id 3 (source operator, recorded 2026-09-01"));
    assert!(input.contains(&long.text));
}

#[test]
fn input_without_neighbours_says_so_and_carries_the_shown_documents() {
    let input = build_input("New rule text", &[], &[skill_row()], 500);
    assert!(input.contains("(none close enough to compare)"));
    assert!(input.contains("skill file deploy-the-proxy/SKILL.md"));
}

#[test]
fn answer_parses_the_schema_shape() -> Result<(), serde_json::Error> {
    let parsed: RelateAnswer = serde_json::from_value(json!({
        "decisions": [{"lesson_id": 4, "decision": "merge", "reason": "complementary"}],
        "replacement": "Do the thing.",
        "placement": {"home": "skill", "covered_by": "", "reason": "steps", "persona": false},
    }))?;
    assert_eq!(parsed.decisions[0].decision, Relation::Merge);
    assert_eq!(parsed.placement.as_ref().map(|p| p.home), Some(Home::Skill));
    assert_eq!(schema()["required"].as_array().map(Vec::len), Some(3));
    Ok(())
}

#[test]
fn an_answer_without_a_placement_still_parses() -> Result<(), serde_json::Error> {
    let parsed: RelateAnswer = serde_json::from_value(json!({
        "decisions": [],
        "replacement": "",
    }))?;
    assert_eq!(parsed.placement, None);
    Ok(())
}

#[test]
fn an_unknown_decision_word_fails_to_parse() {
    let parsed: Result<RelateAnswer, _> = serde_json::from_value(json!({
        "decisions": [{"lesson_id": 4, "decision": "delete", "reason": "x"}],
        "replacement": "",
    }));
    assert!(parsed.is_err());
}
