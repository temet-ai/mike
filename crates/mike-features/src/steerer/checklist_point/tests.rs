use std::collections::BTreeMap;

use super::compose::compose;
use super::questions::{key, questions};
use super::state::{Criterion, Evidence, parse_criteria, state};
use crate::llm::decisions::Answer;
use crate::llm::decisions::point::Asked;
use crate::steerer::verdict::{Need, SteererVerdict};

fn criterion(text: &str, inferred: bool) -> Criterion {
    Criterion {
        text: text.to_owned(),
        inferred,
        operator_words: None,
    }
}

#[test]
fn parse_criteria_thread_block_keeps_list_items_or_every_plain_line() {
    let headed = parse_criteria(
        Some("Acceptance criteria:\n- It ships.\n- It is proven."),
        None,
    );
    assert_eq!(
        headed,
        vec![
            criterion("It ships.", false),
            criterion("It is proven.", false)
        ]
    );
    let plain = parse_criteria(Some("It ships.\nIt is proven."), None);
    assert_eq!(
        plain,
        vec![
            criterion("It ships.", false),
            criterion("It is proven.", false)
        ]
    );
}

#[test]
fn parse_criteria_rendered_intake_map_gives_one_criterion_per_row() {
    use crate::steerer::fleet::verifier_judge::render_intake_criteria;
    use crate::store::persistence::acceptance_criteria::{
        AcceptanceCriteriaMap, AcceptanceCriterionRow,
    };
    let row = |ord: i32, text: &str, span: Option<&str>| AcceptanceCriterionRow {
        ord,
        criterion: text.to_owned(),
        provenance: if span.is_some() {
            "operator"
        } else {
            "inferred"
        }
        .to_owned(),
        operator_span: span.map(str::to_owned),
    };
    let map = AcceptanceCriteriaMap {
        id: 1,
        version: 1,
        ask_entry_count: 3,
        model: None,
        output_tokens: None,
        created_at: chrono::Utc::now(),
        criteria: vec![
            row(0, "The panel is reachable.", Some("no direct URL")),
            row(1, "It uses shared primitives.", None),
            row(2, "It loads under a second.", Some("make it fast")),
        ],
    };
    let found = parse_criteria(None, Some(&render_intake_criteria(&map)));
    let words = |w: &str| Some(w.to_owned());
    assert_eq!(
        found,
        vec![
            Criterion {
                operator_words: words("no direct URL"),
                ..criterion("The panel is reachable.", false)
            },
            criterion("It uses shared primitives.", true),
            Criterion {
                operator_words: words("make it fast"),
                ..criterion("It loads under a second.", false)
            },
        ]
    );
}

fn answered(ps: &[f64]) -> Asked {
    Asked::answered(
        ps.iter()
            .enumerate()
            .map(|(i, p)| {
                (
                    key(i),
                    Answer::Noul {
                        noul: *p,
                        confidence: None,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>(),
    )
}

fn writer(fires: bool, nudge: &str) -> SteererVerdict {
    SteererVerdict {
        fires,
        confidence: 0.4,
        reason: "writer reason".to_owned(),
        nudge: nudge.to_owned(),
        need: Need::Medium,
        checklist: Vec::new(),
    }
}

#[test]
fn parse_criteria_reads_list_markers_and_the_inferred_tag() {
    let found = parse_criteria(
        None,
        Some(
            "- The page loads.\n2. It renders.\n- [inferred, not stated by the operator] It is fast.\n\n",
        ),
    );
    assert_eq!(
        found,
        vec![
            criterion("The page loads.", false),
            criterion("It renders.", false),
            criterion("It is fast.", true),
        ]
    );
}

#[test]
fn parse_criteria_prefers_the_thread_criteria_over_the_intake_map() {
    let found = parse_criteria(Some("- From the thread."), Some("- From intake."));
    assert_eq!(found, vec![criterion("From the thread.", false)]);
}

#[test]
fn state_carries_the_end_of_the_digest_and_every_criterion() {
    let long = format!("{}LAST CALL", "x".repeat(9_000));
    let value = state(
        &[criterion("It loads", false)],
        &Evidence {
            trajectory: &long,
            turn_end: "Done.",
        },
    );
    let trajectory = value["evidence"]["trajectory"].as_str().unwrap_or_default();
    assert!(trajectory.ends_with("LAST CALL"));
    assert_eq!(value["criteria"][key(0)], "It loads");
    assert_eq!(questions(1).len(), 1);
}

#[test]
fn compose_fires_when_a_stated_criterion_is_not_shown_done() {
    let criteria = [criterion("A", false), criterion("B", false)];
    let Some(list) = compose(&answered(&[0.9, 0.1]), &criteria) else {
        panic!("answered questions must compose");
    };
    assert!(list.fires());
    assert_eq!(list.open_stated().len(), 1);
    assert!((list.confidence() - 0.9).abs() < 1e-9);
    let verdict = list.verdict();
    assert!(verdict.fires);
    assert_eq!(verdict.need, Need::High);
    assert_eq!(verdict.checklist[1].status, "not_done");
    assert!(verdict.nudge.contains("\"B\""));
}

#[test]
fn compose_does_not_fire_on_an_inferred_criterion_alone() {
    let criteria = [criterion("A", false), criterion("B", true)];
    let Some(list) = compose(&answered(&[0.95, 0.05]), &criteria) else {
        panic!("answered questions must compose");
    };
    assert!(!list.fires());
    let verdict = list.verdict();
    assert!(!verdict.fires);
    assert!(verdict.nudge.is_empty());
}

#[test]
fn merged_keeps_the_writers_nudge_and_the_checklists_decision() {
    let criteria = [criterion("A", false)];
    let Some(list) = compose(&answered(&[0.1]), &criteria) else {
        panic!("answered questions must compose");
    };
    let merged = list.merged(&writer(false, "Where is A?"));
    assert!(merged.fires);
    assert_eq!(merged.nudge, "Where is A?");
    let fallback = list.merged(&writer(false, "  "));
    assert!(fallback.nudge.contains("\"A\""));
}

#[test]
fn compose_failed_or_incomplete_call_gives_the_off_path_result() {
    let criteria = [criterion("A", false), criterion("B", false)];
    assert!(compose(&Asked::failed_with("timeout"), &criteria).is_none());
    assert!(compose(&answered(&[0.9]), &criteria).is_none());
}
