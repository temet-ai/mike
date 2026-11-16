use std::collections::BTreeMap;

use super::compose::{Status, compose};
use super::questions::{
    BROKEN, CLAIMS_FINISHED, DONE, MISSING, NOT_SHOWN_BROKEN, OPEN_STATED, key, questions,
};
use super::state::Criterion;
use crate::llm::decisions::point::Asked;
use crate::llm::decisions::{Answer, Question};
use crate::steerer::verdict::{Need, SteererVerdict};

fn criterion(text: &str, inferred: bool) -> Criterion {
    Criterion {
        text: text.to_owned(),
        inferred,
        context: None,
    }
}

fn answered(choices: &[(&str, f64)]) -> Asked {
    answered_claiming(choices, 0.9)
}

fn answered_claiming(choices: &[(&str, f64)], claims_finished: f64) -> Asked {
    let mut answers = choices
        .iter()
        .enumerate()
        .map(|(i, (choice, p))| {
            (
                key(i),
                Answer::Choice {
                    choice: (*choice).to_owned(),
                    probabilities: BTreeMap::from([((*choice).to_owned(), *p)]),
                    confidence: None,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    answers.insert(
        CLAIMS_FINISHED.to_owned(),
        Answer::Noul {
            noul: claims_finished,
            confidence: None,
        },
    );
    Asked::answered(answers)
}

#[test]
fn compose_a_missing_goal_does_not_fire_unless_the_turn_claims_the_task_finished() {
    let criteria = [
        criterion("A", false),
        criterion("Nothing on omit is touched.", false),
    ];
    let Some(working) = compose(
        &answered_claiming(&[(MISSING, 0.9), (NOT_SHOWN_BROKEN, 0.9)], 0.1),
        &criteria,
    ) else {
        panic!("answered questions must compose");
    };
    assert!(!working.fires());
    let Some(broken) = compose(
        &answered_claiming(&[(MISSING, 0.9), (BROKEN, 0.9)], 0.1),
        &criteria,
    ) else {
        panic!("answered questions must compose");
    };
    assert!(broken.fires());
    assert_eq!(broken.flagged().len(), 1);
}

#[test]
fn questions_ask_whether_the_turn_claims_the_task_finished() {
    let asked = questions(&[criterion("A", false)]);
    assert!(matches!(
        asked.get(CLAIMS_FINISHED),
        Some(Question::Noul { .. })
    ));
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

fn options(question: Option<&Question>) -> Vec<String> {
    match question {
        Some(Question::Choice { criteria, .. }) => criteria.keys().cloned().collect(),
        _ => Vec::new(),
    }
}

#[test]
fn questions_ask_an_outcome_for_a_goal_and_broken_for_a_rule() {
    let asked = questions(&[
        criterion("PR #256 merged and deployed on omit.", false),
        criterion("Nothing on omit is touched.", false),
    ]);
    assert_eq!(
        options(asked.get(&key(0))),
        vec![DONE, MISSING, OPEN_STATED]
    );
    assert_eq!(options(asked.get(&key(1))), vec![BROKEN, NOT_SHOWN_BROKEN]);
}

#[test]
fn compose_fires_when_a_stated_criterion_is_missing_above_the_threshold() {
    let criteria = [criterion("A", false), criterion("B", false)];
    let Some(list) = compose(&answered(&[(DONE, 0.9), (MISSING, 0.8)]), &criteria) else {
        panic!("answered questions must compose");
    };
    assert!(list.fires());
    assert_eq!(list.flagged().len(), 1);
    assert!((list.confidence() - 0.8).abs() < 1e-9);
    let verdict = list.verdict();
    assert!(verdict.fires);
    assert_eq!(verdict.need, Need::High);
    assert_eq!(verdict.checklist[0].status, "done");
    assert_eq!(verdict.checklist[1].status, "not_done");
    assert!(verdict.nudge.contains("\"B\""));
}

#[test]
fn compose_missing_below_the_threshold_does_not_fire() {
    let criteria = [criterion("A", false)];
    let Some(list) = compose(&answered(&[(MISSING, 0.55)]), &criteria) else {
        panic!("answered questions must compose");
    };
    assert!(!list.fires());
}

#[test]
fn compose_open_stated_never_fires() {
    let criteria = [criterion("A", false), criterion("B", false)];
    let Some(list) = compose(&answered(&[(OPEN_STATED, 0.99), (DONE, 0.7)]), &criteria) else {
        panic!("answered questions must compose");
    };
    assert!(!list.fires());
    assert_eq!(list.open_stated().len(), 1);
    let verdict = list.verdict();
    assert!(verdict.nudge.is_empty());
    assert_eq!(verdict.checklist[0].status, "partial");
    assert!(verdict.reason.contains("1 stated as still open"));
}

#[test]
fn compose_fires_on_a_broken_rule_with_the_rule_nudge() {
    let criteria = [
        criterion("Done.", false),
        criterion("No secrets are logged.", false),
    ];
    let Some(list) = compose(&answered(&[(DONE, 0.9), (BROKEN, 0.75)]), &criteria) else {
        panic!("answered questions must compose");
    };
    assert!(list.fires());
    assert_eq!(list.items[1].status, Status::Broken);
    assert!(
        list.verdict()
            .nudge
            .contains("rule \"No secrets are logged.\"")
    );
    let Some(kept) = compose(
        &answered(&[(DONE, 0.9), (NOT_SHOWN_BROKEN, 0.9)]),
        &criteria,
    ) else {
        panic!("answered questions must compose");
    };
    assert!(!kept.fires());
}

#[test]
fn compose_does_not_fire_on_an_inferred_criterion_alone() {
    let criteria = [criterion("A", false), criterion("B", true)];
    let Some(list) = compose(&answered(&[(DONE, 0.95), (MISSING, 0.95)]), &criteria) else {
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
    let Some(list) = compose(&answered(&[(MISSING, 0.9)]), &criteria) else {
        panic!("answered questions must compose");
    };
    let merged = list.merged(&writer(false, "Where is A?"));
    assert!(merged.fires);
    assert_eq!(merged.nudge, "Where is A?");
    let fallback = list.merged(&writer(false, "  "));
    assert!(fallback.nudge.contains("\"A\""));
}

#[test]
fn compose_failed_incomplete_or_mismatched_call_gives_the_off_path_result() {
    let criteria = [criterion("A", false), criterion("B", false)];
    assert!(compose(&Asked::failed_with("timeout"), &criteria).is_none());
    assert!(compose(&answered(&[(DONE, 0.9)]), &criteria).is_none());
    assert!(compose(&answered(&[(DONE, 0.9), (BROKEN, 0.9)]), &criteria).is_none());
}
