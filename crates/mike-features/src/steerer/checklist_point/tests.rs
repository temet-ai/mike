use std::collections::BTreeMap;

use super::compose::compose;
use super::questions::{key, questions};
use super::state::{Criterion, Evidence, session_criteria, state, thread_criteria};
use crate::llm::decisions::Answer;
use crate::llm::decisions::point::Asked;
use crate::steerer::verdict::{Need, SteererVerdict};
use crate::store::persistence::acceptance_criteria::{
    AcceptanceCriteriaMap, AcceptanceCriterionRow,
};

fn criterion(text: &str, inferred: bool) -> Criterion {
    Criterion {
        text: text.to_owned(),
        inferred,
    }
}

fn intake_map(rows: &[(&str, Option<&str>)]) -> AcceptanceCriteriaMap {
    AcceptanceCriteriaMap {
        id: 1,
        version: 1,
        ask_entry_count: 1,
        model: None,
        output_tokens: None,
        created_at: chrono::Utc::now(),
        criteria: rows
            .iter()
            .zip(0..)
            .map(|((text, span), ord)| AcceptanceCriterionRow {
                ord,
                criterion: (*text).to_owned(),
                provenance: if span.is_some() {
                    "operator"
                } else {
                    "inferred"
                }
                .to_owned(),
                operator_span: span.map(str::to_owned),
            })
            .collect(),
    }
}

#[test]
fn session_criteria_intake_map_gives_one_criterion_per_stored_row() {
    let map = intake_map(&[
        ("The panel is reachable.", Some("no direct URL")),
        ("It uses shared primitives.", None),
        ("  ", None),
        ("It loads under a second.", Some("make it fast")),
    ]);
    assert_eq!(
        session_criteria(None, Some(&map)),
        vec![
            criterion("The panel is reachable.", false),
            criterion("It uses shared primitives.", true),
            criterion("It loads under a second.", false),
        ]
    );
    assert!(session_criteria(None, None).is_empty());
}

#[test]
fn thread_criteria_keeps_list_items_or_every_plain_line() {
    let headed = thread_criteria("Acceptance criteria:\n- It ships.\n- It is proven.");
    assert_eq!(
        headed,
        vec![
            criterion("It ships.", false),
            criterion("It is proven.", false)
        ]
    );
    let plain = thread_criteria("It ships.\nIt is proven.");
    assert_eq!(
        plain,
        vec![
            criterion("It ships.", false),
            criterion("It is proven.", false)
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
fn thread_criteria_reads_list_markers_and_the_inferred_tag() {
    let found = thread_criteria(
        "- The page loads.\n2. It renders.\n- [inferred, not stated by the operator] It is fast.\n\n",
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
fn session_criteria_linked_thread_beats_the_intake_map() {
    let map = intake_map(&[("From intake.", None)]);
    let found = session_criteria(Some("- From the thread."), Some(&map));
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
