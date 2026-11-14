use std::collections::BTreeMap;

use super::compose::compose;
use super::questions::{DISCLOSED, ONLY_ANSWER, UNFINISHED};
use super::state::{FireInput, state};
use crate::llm::decisions::Answer;
use crate::llm::decisions::point::Asked;
use crate::steerer::verdict::{ChecklistItem, SteererVerdict};

fn answered(unfinished: f64, disclosed: f64, only: f64) -> Asked {
    let noul = |p| Answer::Noul {
        noul: p,
        confidence: None,
    };
    Asked::answered(BTreeMap::from([
        (UNFINISHED.to_owned(), noul(unfinished)),
        (DISCLOSED.to_owned(), noul(disclosed)),
        (ONLY_ANSWER.to_owned(), noul(only)),
    ]))
}

fn verdict() -> SteererVerdict {
    SteererVerdict {
        fires: true,
        confidence: 0.9,
        reason: "ask not done".to_owned(),
        nudge: "you did not do it".to_owned(),
        need: crate::steerer::verdict::Need::High,
        checklist: vec![ChecklistItem {
            criterion: "fix the clearance".to_owned(),
            status: "not_done".to_owned(),
            evidence: "x".to_owned(),
        }],
    }
}

#[test]
fn compose_real_gap_not_disclosed_is_sent() {
    assert_eq!(
        compose(&answered(0.9, 0.1, 0.1)).map(|v| v.send),
        Some(true)
    );
}

#[test]
fn compose_gap_already_disclosed_in_an_answer_is_held() {
    assert_eq!(
        compose(&answered(0.05, 0.9, 0.9)).map(|v| v.send),
        Some(false)
    );
}

#[test]
fn compose_sends_only_a_clearly_unfinished_ask_whatever_the_other_answers_say() {
    assert_eq!(
        compose(&answered(0.7, 0.0, 0.0)).map(|v| v.send),
        Some(false)
    );
    assert_eq!(
        compose(&answered(0.85, 0.9, 0.9)).map(|v| v.send),
        Some(true)
    );
}

#[test]
fn compose_failed_ask_is_none_so_the_fire_is_sent_as_today() {
    assert!(compose(&Asked::failed_with("timeout")).is_none());
}

#[test]
fn state_carries_the_checklist_status_and_the_turn_end() {
    let v = verdict();
    let long = format!("{}All done and tested.", "q".repeat(9_000));
    let input = FireInput {
        operator_ask: "fix the clearance",
        turn_end: &long,
        nudge: &v.nudge,
        verdict: &v,
    };
    let s = state(&input);
    assert_eq!(s["checklist"][0]["status"], "not_done");
    assert!(
        s["turn_end"]
            .as_str()
            .unwrap_or_default()
            .ends_with("All done and tested.")
    );
}
