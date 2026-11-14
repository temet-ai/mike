use std::collections::BTreeMap;

use super::compose::compose;
use super::questions::{key, questions};
use super::state::{Candidate, Reason, state};
use super::{pending, remember, verdict};
use crate::llm::decisions::Answer;
use crate::llm::decisions::point::Asked;

fn candidate(session_id: i64, prompt: &str, reason: Reason) -> Candidate {
    Candidate {
        session_id,
        first_prompt: prompt.to_owned(),
        project_path: Some("/work/app".to_owned()),
        reason,
    }
}

fn answered(pairs: &[(i64, f64)]) -> Asked {
    Asked::answered(
        pairs
            .iter()
            .map(|(id, p)| {
                (
                    key(*id),
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
fn questions_ask_one_noul_per_candidate_session() {
    let a = candidate(1, "Reply with exactly OK", Reason::RegexMatch);
    let b = candidate(2, "hi", Reason::ShortSingleTurn);
    let asked = questions(&[&a, &b]);
    assert_eq!(asked.len(), 2);
    assert!(asked.contains_key(&key(1)));
    assert!(asked.contains_key(&key(2)));
}

#[test]
fn state_carries_the_prompt_and_whether_the_pattern_matched() {
    let a = candidate(1, "Reply with exactly OK", Reason::RegexMatch);
    let value = state(&[&a]);
    assert_eq!(
        value["sessions"][key(1)]["probe_pattern_matched"],
        serde_json::json!(true)
    );
    assert_eq!(
        value["sessions"][key(1)]["first_prompt"],
        serde_json::json!("Reply with exactly OK")
    );
}

#[test]
fn compose_marks_a_session_a_probe_at_or_above_one_half() {
    let a = candidate(1, "x", Reason::RegexMatch);
    let b = candidate(2, "y", Reason::RegexMatch);
    let found = compose(&answered(&[(1, 0.9), (2, 0.2)]), &[&a, &b]);
    assert_eq!(found.len(), 2);
    assert!(found[0].is_probe);
    assert!(!found[1].is_probe);
    assert!((found[1].score() - 0.8).abs() < 1e-9);
}

#[test]
fn compose_failed_call_gives_no_verdicts() {
    let a = candidate(1, "x", Reason::RegexMatch);
    assert!(compose(&Asked::failed_with("timeout"), &[&a]).is_empty());
}

#[test]
fn remember_then_verdict_returns_the_answer_and_pending_skips_known_sessions() {
    let id = 987_654_001;
    remember(&[super::Verdict {
        session_id: id,
        is_probe: true,
        probability: 0.9,
    }]);
    assert_eq!(verdict(id), Some(true));
    let left = pending(vec![
        candidate(id, "x", Reason::RegexMatch),
        candidate(id + 1, "y", Reason::RegexMatch),
    ]);
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].session_id, id + 1);
}
