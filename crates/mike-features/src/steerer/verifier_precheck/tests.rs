use std::collections::BTreeMap;

use super::compose::compose;
use super::facts::{AskSource, Facts, NudgeSource};
use super::questions::{
    CRITERION_GOAL, DISCLOSED, NOT_OPERATOR_ASK, STILL_RUNNING, UNFINISHED, VETOES, questions,
    questions_for,
};
use super::state::{FireInput, state, state_with};
use crate::llm::decisions::Answer;
use crate::llm::decisions::point::Asked;
use crate::steerer::verdict::{ChecklistItem, SteererVerdict};

fn noul(p: f64) -> Answer {
    Answer::Noul {
        noul: p,
        confidence: None,
    }
}

fn answered(unfinished: f64, disclosed: f64) -> Asked {
    Asked::answered(BTreeMap::from([
        (UNFINISHED.to_owned(), noul(unfinished)),
        (DISCLOSED.to_owned(), noul(disclosed)),
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
    assert_eq!(compose(&answered(0.9, 0.1)).map(|v| v.send), Some(true));
}

#[test]
fn compose_gap_the_turn_already_discloses_is_held_even_when_unfinished() {
    let verdict = compose(&answered(0.95, 0.84));
    assert_eq!(verdict.as_ref().map(|v| v.send), Some(false));
    assert!(verdict.is_some_and(|v| v.score < 0.2));
}

#[test]
fn compose_sends_only_a_clearly_unfinished_ask() {
    assert_eq!(compose(&answered(0.7, 0.0)).map(|v| v.send), Some(false));
    assert_eq!(compose(&answered(0.85, 0.1)).map(|v| v.send), Some(true));
}

fn with_veto(key: &str, p: f64) -> Asked {
    Asked::answered(BTreeMap::from([
        (UNFINISHED.to_owned(), noul(0.95)),
        (key.to_owned(), noul(p)),
    ]))
}

#[test]
fn compose_unfinished_ask_with_any_veto_is_held() {
    for key in VETOES {
        let verdict = compose(&with_veto(key, 0.8));
        assert_eq!(verdict.as_ref().map(|v| v.send), Some(false), "{key}");
        assert!(verdict.is_some_and(|v| v.score < 0.3), "{key}");
    }
}

#[test]
fn compose_unfinished_ask_with_weak_vetoes_is_sent() {
    assert_eq!(
        compose(&with_veto(STILL_RUNNING, 0.2)).map(|v| v.send),
        Some(true)
    );
}

#[test]
fn compose_criterion_the_operator_never_stated_is_held_and_a_stated_one_is_sent() {
    assert_eq!(
        compose(&with_veto(CRITERION_GOAL, 0.2)).map(|v| v.send),
        Some(false)
    );
    assert_eq!(
        compose(&with_veto(CRITERION_GOAL, 0.9)).map(|v| v.send),
        Some(true)
    );
}

#[test]
fn compose_failed_ask_is_none_so_the_fire_is_sent_as_today() {
    assert!(compose(&Asked::failed_with("timeout")).is_none());
}

#[test]
fn questions_for_a_criteria_nudge_ask_the_goal_match_not_the_operator_words_veto() {
    let criteria = questions_for(NudgeSource::Criteria);
    assert!(criteria.contains_key(CRITERION_GOAL));
    assert!(!criteria.contains_key(NOT_OPERATOR_ASK));
    let verifier = questions();
    assert!(verifier.contains_key(NOT_OPERATOR_ASK));
    assert!(!verifier.contains_key(CRITERION_GOAL));
    for set in [criteria, verifier] {
        assert!(set.contains_key(DISCLOSED));
        assert!(!set.contains_key("turn_only_answers"));
    }
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

#[test]
fn state_with_facts_carries_the_sources_and_the_recent_commits() {
    let v = verdict();
    let input = FireInput {
        operator_ask: "",
        turn_end: "done",
        nudge: &v.nudge,
        verdict: &v,
    };
    let facts = Facts {
        nudge_source: NudgeSource::Verifier,
        judged_ask: "[initial] fix the clearance".to_owned(),
        ask_source: AskSource::Operator,
        recent_commits: vec!["7d199613c Fix the clearance".to_owned()],
    };
    let s = state_with(&input, &facts);
    assert_eq!(s["nudge_source"], "verifier");
    assert_eq!(s["ask_source"], "operator");
    assert_eq!(s["judged_ask"], "[initial] fix the clearance");
    assert_eq!(s["recent_commits"][0], "7d199613c Fix the clearance");
    assert_eq!(s["checklist"][0]["status"], "not_done");
}
