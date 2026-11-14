use std::collections::BTreeMap;

use chrono::Utc;

use super::compose::{compose, keep, mean_confidence};
use super::questions::{key, questions};
use super::state::{sentence_around, state};
use crate::llm::decisions::Answer;
use crate::llm::decisions::point::Asked;
use crate::store::models::{Diagnostic, DiagnosticCategory};

fn diagnostic(text: &str, phrase: &str) -> Diagnostic {
    let start = text.find(phrase).unwrap_or(0);
    Diagnostic {
        category: DiagnosticCategory::AvoidingWork,
        analyzer: "avoiding_work".to_owned(),
        advice: "advice".to_owned(),
        matched_text: phrase.to_owned(),
        match_range: (start, start + phrase.len()),
        other_hits: 0,
        created_at: Utc::now(),
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

#[test]
fn sentence_around_returns_the_sentence_holding_the_match() {
    let text = "Tests pass. That part is out of scope for this task. Next steps follow.";
    let start = text.find("out of scope").unwrap_or(0);
    assert_eq!(
        sentence_around(text, (start, start + 12)),
        "That part is out of scope for this task."
    );
}

#[test]
fn sentence_around_survives_multibyte_text_and_a_bad_range() {
    let text = "Das ist für jetzt gut. Wir machen das for now.";
    let start = text.find("for now").unwrap_or(0);
    assert_eq!(
        sentence_around(text, (start, start + 7)),
        "Wir machen das for now."
    );
    assert_eq!(sentence_around(text, (1000, 2000)), text);
}

#[test]
fn state_and_questions_are_keyed_by_the_same_match_names() {
    let text = "It is out of scope.";
    let diagnostics = [diagnostic(text, "out of scope")];
    let value = state(text, &diagnostics);
    assert_eq!(value["matches"][key(0)]["phrase"], "out of scope");
    assert_eq!(value["matches"][key(0)]["category"], "avoiding_work");
    assert!(questions(1).contains_key(&key(0)));
}

#[test]
fn compose_keeps_the_excuse_and_drops_the_ordinary_use() {
    let excuses = compose(&answered(&[0.9, 0.1]), 2);
    assert_eq!(excuses, Some(vec![true, false]));
    let kept = keep(vec!["excuse", "ordinary"], &[true, false]);
    assert_eq!(kept, vec!["excuse"]);
}

#[test]
fn compose_failed_call_keeps_every_finding() {
    assert_eq!(compose(&Asked::failed_with("timeout"), 2), None);
    assert_eq!(keep(vec![1, 2], &[]), vec![1, 2]);
}

#[test]
fn mean_confidence_averages_the_stronger_side_of_each_answer() {
    assert!((mean_confidence(&answered(&[0.9, 0.2]), 2) - 0.85).abs() < 1e-9);
    assert_eq!(mean_confidence(&answered(&[]), 0), 0.0);
}
