use std::collections::{BTreeMap, HashMap};

use chrono::Utc;

use super::compose::{Decision, compose, rewrite};
use super::questions::{key, questions};
use super::state::{Unit, sentence_around, state, units};
use crate::dossier::entries::directives::entries_for;
use crate::llm::decisions::Answer;
use crate::llm::decisions::point::Asked;
use crate::store::persistence::dossier_entries::DirectiveSourceRow;

fn source(text: &str) -> DirectiveSourceRow {
    DirectiveSourceRow {
        turn_id: 11,
        session_id: 3,
        start_request_id: 500,
        user_prompt: text.to_owned(),
        created_at: Utc::now(),
    }
}

fn answer(index: usize, choice: &str, p: f64) -> (String, Answer) {
    (
        key(index),
        Answer::Choice {
            choice: choice.to_owned(),
            probabilities: BTreeMap::from([(choice.to_owned(), p)]),
            confidence: None,
        },
    )
}

fn unit_for(text: &str) -> Vec<Unit> {
    units(&[source(text)])
}

#[test]
fn sentence_around_cuts_at_the_neighbouring_sentence_ends() {
    let text = "First thing. Please push the button on the form. Last thing.";
    let at = text.find("push").unwrap_or(0);
    let found = sentence_around(text, (at, at + 4));
    assert_eq!(found, "Please push the button on the form.");
}

#[test]
fn units_hold_one_unit_per_class_found_with_its_sentence() {
    let found = unit_for("Fix the bug. Don't commit yet. Then push when green.");
    let classes: Vec<&str> = found.iter().map(|u| u.class).collect();
    assert!(classes.contains(&"commit"));
    assert!(classes.contains(&"push"));
    let commit = found.iter().find(|u| u.class == "commit");
    assert_eq!(
        commit.map(|u| u.sentence.as_str()),
        Some("Don't commit yet.")
    );
}

#[test]
fn state_and_questions_are_keyed_by_the_same_unit_names() {
    let found = unit_for("Don't commit yet. Then push when green.");
    let refs: Vec<&Unit> = found.iter().collect();
    let value = state(&refs);
    let asked = questions(&refs);
    for i in 0..refs.len() {
        assert!(value["units"].get(key(i)).is_some());
        assert!(asked.contains_key(&key(i)));
    }
}

#[test]
fn compose_neither_answer_is_decided_and_low_confidence_is_left_to_the_regex() {
    let found = unit_for("Please push the button. Don't commit yet.");
    let refs: Vec<&Unit> = found.iter().collect();
    let asked = Asked::answered(BTreeMap::from([
        answer(0, "neither", 0.9),
        answer(1, "forbids", 0.4),
    ]));
    let decided = compose(&asked, &refs);
    assert_eq!(decided.len(), 2);
    assert_eq!(decided[0].decision, Some(Decision::Neither));
    assert_eq!(decided[1].decision, None);
}

#[test]
fn compose_failed_call_decides_nothing() {
    let found = unit_for("Don't commit yet.");
    let refs: Vec<&Unit> = found.iter().collect();
    assert!(compose(&Asked::failed_with("timeout"), &refs).is_empty());
}

#[test]
fn rewrite_drops_the_regex_entry_when_jev_says_neither() {
    let row = source("Please push the button on the form.");
    let regex_entries = entries_for(std::slice::from_ref(&row));
    assert!(regex_entries.iter().any(|e| e.data["class"] == "push"));
    let decided = HashMap::from([(
        (row.turn_id, "push".to_owned()),
        (row.start_request_id, Decision::Neither),
    )]);
    let kept = rewrite(regex_entries, &decided);
    assert!(kept.iter().all(|e| e.data["class"] != "push"));
}

#[test]
fn rewrite_replaces_the_polarity_and_marks_the_entry_as_decided_by_jev() {
    let row = source("Please push the button on the form.");
    let regex_entries = entries_for(std::slice::from_ref(&row));
    let decided = HashMap::from([(
        (row.turn_id, "push".to_owned()),
        (row.start_request_id, Decision::Forbids),
    )]);
    let kept = rewrite(regex_entries, &decided);
    let push: Vec<_> = kept.iter().filter(|e| e.data["class"] == "push").collect();
    assert_eq!(push.len(), 1);
    assert_eq!(push[0].data["polarity"], "forbid");
    assert_eq!(push[0].data["decided_by"], "jev");
    assert_eq!(push[0].dedup_key, "request:500:push:forbid");
}

#[test]
fn rewrite_without_a_decision_returns_the_regex_entries_unchanged() {
    let row = source("Don't commit yet. Then push when green.");
    let regex_entries = entries_for(std::slice::from_ref(&row));
    let kept = rewrite(regex_entries.clone(), &HashMap::new());
    assert_eq!(kept, regex_entries);
}
