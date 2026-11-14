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
    }
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
