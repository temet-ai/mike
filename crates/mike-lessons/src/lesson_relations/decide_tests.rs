use chrono::{TimeZone, Utc};

use super::*;
use crate::llm::decisions::Answer;

fn pair(a_text: &str, b_text: &str) -> CandidateLessonPair {
    CandidateLessonPair {
        lesson_a_id: 10,
        lesson_a_text: a_text.to_owned(),
        lesson_a_created_at: Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap(),
        lesson_b_id: 20,
        lesson_b_text: b_text.to_owned(),
        lesson_b_created_at: Utc.with_ymd_and_hms(2026, 11, 5, 0, 0, 0).unwrap(),
        distance: 0.12,
    }
}

fn pick(key: &str, p: f64) -> Answer {
    Answer::Choice {
        choice: key.to_owned(),
        probabilities: BTreeMap::from([(key.to_owned(), p)]),
        confidence: None,
    }
}

fn relation(key: &str, p: f64) -> Asked {
    Asked::answered(BTreeMap::from([(RELATION_KEY.to_owned(), pick(key, p))]))
}

#[test]
fn compose_supersedes_runs_from_the_newer_lesson_to_the_older() {
    let p = pair("old text", "new text");
    let decision = compose(&relation(KIND_SUPERSEDES, 0.9), &p, None);
    assert_eq!(
        decision,
        Some(PairDecision::Relation {
            from_lesson_id: 20,
            to_lesson_id: 10,
            kind: KIND_SUPERSEDES,
            probability: 0.9,
        })
    );
}

#[test]
fn compose_unrelated_or_unsure_writes_no_relation() {
    let p = pair("a", "b");
    assert_eq!(
        compose(&relation(UNRELATED, 0.9), &p, None),
        Some(PairDecision::Unrelated(0.9))
    );
    assert_eq!(
        compose(&relation(KIND_REFINES, 0.3), &p, None),
        Some(PairDecision::Unrelated(0.3))
    );
}

#[test]
fn compose_failed_answer_gives_the_off_path() {
    let p = pair("a", "b");
    assert_eq!(compose(&Asked::failed_with("timeout"), &p, None), None);
}

#[test]
fn cite_of_finds_the_lesson_id_and_jev_decides_whether_it_is_one() {
    let p = pair("plain", "see lesson #10 for the earlier rule");
    let cite = cite_of(&p);
    assert_eq!(
        cite.as_ref().map(|c| (c.from_lesson_id, c.to_lesson_id)),
        Some((20, 10))
    );
    let issue_number = Asked::answered(BTreeMap::from([
        (RELATION_KEY.to_owned(), pick(UNRELATED, 0.8)),
        (
            CITES_KEY.to_owned(),
            Answer::Noul {
                noul: 0.1,
                confidence: None,
            },
        ),
    ]));
    assert_eq!(
        compose(&issue_number, &p, cite.as_ref()),
        Some(PairDecision::Unrelated(0.8))
    );
    let real_cite = Asked::answered(BTreeMap::from([
        (RELATION_KEY.to_owned(), pick(UNRELATED, 0.8)),
        (
            CITES_KEY.to_owned(),
            Answer::Noul {
                noul: 0.95,
                confidence: None,
            },
        ),
    ]));
    assert!(matches!(
        compose(&real_cite, &p, cite.as_ref()),
        Some(PairDecision::Cites(_))
    ));
}

#[test]
fn questions_and_state_carry_the_pair_facts() {
    let p = pair("old FIXED text", "new text");
    let cite = cite_of(&p);
    let asked = questions(&p, cite.as_ref());
    assert!(asked.contains_key(RELATION_KEY));
    assert!(!asked.contains_key(CITES_KEY));
    let value = state(&p, cite.as_ref());
    assert_eq!(value["newer_lesson_id"], 20);
    assert_eq!(value["lesson_a"]["polarity_markers"][0], "FIXED");
}
