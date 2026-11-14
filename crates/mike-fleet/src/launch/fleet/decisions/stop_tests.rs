use std::collections::BTreeMap;

use super::*;
use crate::llm::decisions::Answer;

fn facts(uncommitted: usize, passed_after: bool, running: bool) -> StopFacts {
    StopFacts {
        seat: "worker-T1".into(),
        task: "T1".into(),
        uncommitted: (0..uncommitted).map(|i| format!("src/f{i}.rs")).collect(),
        last_change_at: None,
        check_passed_after_change: passed_after,
        last_check: None,
        open_asks: Vec::new(),
        check_running: running,
        done_when: None,
    }
}

fn noul(value: f64) -> Answer {
    Answer::Noul {
        noul: value,
        confidence: None,
    }
}

fn answered(claims: f64, waiting: f64, unanswered: f64) -> Asked {
    Asked::answered(BTreeMap::from([
        (CLAIMS_DONE.to_owned(), noul(claims)),
        (WAITING.to_owned(), noul(waiting)),
        (UNANSWERED.to_owned(), noul(unanswered)),
    ]))
}

fn first_stop() -> StopRequest {
    StopRequest::default()
}

fn second_stop() -> StopRequest {
    StopRequest {
        final_message: None,
        stop_hook_active: true,
        probe: false,
    }
}

#[test]
fn code_step_a_check_passed_after_the_change_lets_the_stop_through() {
    let step = code_step(&facts(2, true, false), &first_stop());
    assert!(matches!(step, Step::Settled(Decision::LetThrough, _)));
}

#[test]
fn code_step_unchecked_uncommitted_changes_push_back_without_asking() {
    let step = code_step(&facts(2, false, false), &first_stop());
    assert!(matches!(step, Step::Settled(Decision::PushBack, _)));
}

#[test]
fn code_step_unchecked_changes_while_a_check_runs_let_the_stop_through() {
    let step = code_step(&facts(2, false, true), &first_stop());
    assert!(matches!(step, Step::Settled(Decision::LetThrough, _)));
}

#[test]
fn code_step_unchecked_changes_after_one_push_back_let_the_stop_through() {
    let step = code_step(&facts(2, false, false), &second_stop());
    assert!(matches!(step, Step::Settled(Decision::LetThrough, _)));
}

#[test]
fn code_step_nothing_uncommitted_and_no_passing_check_asks_the_model() {
    assert_eq!(code_step(&facts(0, false, false), &first_stop()), Step::Ask);
}

#[test]
fn compose_a_done_claim_lets_the_stop_through() {
    let (decision, _, _) = compose(&answered(0.9, 0.1, 0.1), &first_stop());
    assert_eq!(decision, Decision::LetThrough);
}

#[test]
fn compose_waiting_lets_the_stop_through() {
    let (decision, _, _) = compose(&answered(0.1, 0.9, 0.1), &first_stop());
    assert_eq!(decision, Decision::LetThrough);
}

#[test]
fn compose_an_unanswered_question_lets_the_stop_through() {
    let (decision, _, _) = compose(&answered(0.1, 0.1, 0.9), &first_stop());
    assert_eq!(decision, Decision::LetThrough);
}

#[test]
fn compose_none_of_the_three_pushes_back_once() {
    let (first, _, _) = compose(&answered(0.1, 0.1, 0.1), &first_stop());
    let (second, _, _) = compose(&answered(0.1, 0.1, 0.1), &second_stop());
    assert_eq!((first, second), (Decision::PushBack, Decision::LetThrough));
}

#[test]
fn compose_a_failed_call_is_the_off_path() {
    let (decision, _, _) = compose(&Asked::failed_with("timeout"), &first_stop());
    assert_eq!(decision, Decision::Off);
}

#[test]
fn questions_leave_out_the_claim_question_without_a_final_message() {
    assert_eq!(questions(false).len(), 2);
    assert_eq!(questions(true).len(), 3);
}
