use super::*;
use crate::llm::decisions::Answer;

fn shown(position: usize) -> Shown<'static> {
    Shown {
        position,
        reference: "ref",
        text: "a document",
        query_score: Some(0.4),
        turn_score: None,
    }
}

fn answer(p: f64) -> Answer {
    Answer::Noul {
        noul: p,
        confidence: None,
    }
}

#[test]
fn compose_gives_every_shown_result_its_probability() {
    let rows = [shown(1), shown(2)];
    let asked = Asked::answered(BTreeMap::from([
        (question_key(1), answer(0.9)),
        (question_key(2), answer(0.2)),
    ]));
    assert_eq!(compose(&asked, &rows), Some(vec![(1, 0.9), (2, 0.2)]));
}

#[test]
fn compose_failed_or_incomplete_answer_keeps_the_threshold_grade() {
    let rows = [shown(1), shown(2)];
    assert_eq!(compose(&Asked::failed_with("timeout"), &rows), None);
    let partial = Asked::answered(BTreeMap::from([(question_key(1), answer(0.9))]));
    assert_eq!(compose(&partial, &rows), None);
}

#[test]
fn mean_applied_scores_the_action_taken_for_each_result() {
    let score = mean_applied(&[(1, 0.9), (2, 0.2)]);
    assert!((score - 0.85).abs() < 1e-9);
}

#[test]
fn questions_and_state_cover_each_result_and_the_turn() {
    let rows = [shown(1), shown(2)];
    let asked = questions(&rows, true);
    assert!(asked.contains_key("result_1") && asked.contains_key("result_2"));
    let value = state("query text", Some("the turn"), &rows);
    assert_eq!(value["query"], "query text");
    assert_eq!(value["results"][1]["position"], 2);
    assert_eq!(value["results"][0]["reranker_score_for_query"], 0.4);
}
