use std::collections::BTreeMap;

use super::state::{NoteInput, state, tail_chars};
use super::{NoteKind, compose};
use crate::llm::decisions::Answer;
use crate::llm::decisions::point::Asked;

fn asked(about: f64, said_open: f64) -> Asked {
    let noul = |p: f64| Answer::Noul {
        noul: p,
        confidence: None,
    };
    Asked::answered(BTreeMap::from([
        ("about_this_work".to_owned(), noul(about)),
        ("said_open".to_owned(), noul(said_open)),
    ]))
}

#[test]
fn compose_note_about_other_work_is_held() {
    let gated = compose(&asked(0.2, 0.1), NoteKind::Rule);
    assert_eq!(gated.map(|g| g.send), Some(false));
}

#[test]
fn compose_rule_note_about_this_work_is_sent_even_when_the_turn_says_open() {
    let gated = compose(&asked(0.9, 0.9), NoteKind::Rule);
    assert_eq!(gated.map(|g| g.send), Some(true));
}

#[test]
fn compose_ask_note_the_turn_already_names_open_is_held() {
    assert_eq!(
        compose(&asked(0.9, 0.8), NoteKind::Ask).map(|g| g.send),
        Some(false)
    );
    assert_eq!(
        compose(&asked(0.9, 0.5), NoteKind::Ask).map(|g| g.send),
        Some(true)
    );
}

#[test]
fn compose_failed_ask_is_none() {
    assert!(compose(&Asked::failed_with("timeout"), NoteKind::Ask).is_none());
}

#[test]
fn state_numbers_the_last_calls_and_keeps_the_turn_end() {
    let lines: Vec<String> = (0..45).map(|i| format!("Bash: step {i}")).collect();
    let ending = format!("{}the end", "x".repeat(2000));
    let input = NoteInput {
        kind: NoteKind::Ask,
        note: "You asked for the export.",
        operator_ask: "export it",
        tool_lines: &lines,
        final_text: &ending,
    };
    let s = state(&input);
    let calls = s["tool_calls"].as_array().cloned().unwrap_or_default();
    assert_eq!(calls.len(), 40);
    assert_eq!(calls[0], "T00| Bash: step 5");
    assert!(
        s["final_text"]
            .as_str()
            .is_some_and(|t| t.ends_with("the end"))
    );
    assert_eq!(
        s["final_text"].as_str().map(|t| t.chars().count()),
        Some(1500)
    );
}

#[test]
fn tail_chars_keeps_short_text_whole() {
    assert_eq!(tail_chars("done", 10), "done");
}
