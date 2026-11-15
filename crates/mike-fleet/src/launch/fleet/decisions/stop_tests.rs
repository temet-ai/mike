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
        last_commit_sha: None,
        last_commit_at: None,
        last_report_at: None,
        report_filed_awaiting_verdict: false,
        last_report_refusal: None,
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

fn reported() -> StopFacts {
    StopFacts {
        report_filed_awaiting_verdict: true,
        ..facts(0, false, false)
    }
}

#[test]
fn code_step_a_filed_report_without_a_verdict_lets_the_stop_through_without_asking() {
    match code_step(&reported(), &first_stop()) {
        Step::Settled(Decision::LetThrough, reason) => assert!(reason.contains("verdict")),
        other => panic!("expected a let-through, got {other:?}"),
    }
}

#[test]
fn code_step_uncommitted_unchecked_changes_still_push_back_with_a_report_filed() {
    let facts = StopFacts {
        report_filed_awaiting_verdict: true,
        ..facts(2, false, false)
    };
    let step = code_step(&facts, &first_stop());
    assert!(matches!(step, Step::Settled(Decision::PushBack, _)));
}

#[test]
fn questions_name_a_reply_to_its_mail_and_a_verdict_on_its_report() {
    let asked = questions(false);
    let waiting = &asked[WAITING];
    let text = serde_json::to_string(waiting).unwrap_or_default();
    assert!(text.contains("a reply to its mail, or a verdict on its report"));
}

#[test]
fn message_for_jev_keeps_a_short_message_whole() {
    assert_eq!(message_for_jev("done"), "done");
}

#[test]
fn message_for_jev_keeps_the_head_and_the_tail_of_a_long_report() {
    let long = format!("HEAD{}MIDDLE{}TAIL", "a".repeat(1000), "b".repeat(1000));
    let sent = message_for_jev(&long);
    assert!(sent.starts_with("HEADaaa"));
    assert!(sent.ends_with("bbbTAIL"));
    assert!(!sent.contains("MIDDLE"));
    assert_eq!(sent.len(), MESSAGE_HEAD_BYTES + MESSAGE_TAIL_BYTES);
}

fn after(base: DateTime<Utc>, seconds: i64) -> Option<DateTime<Utc>> {
    Some(base + chrono::Duration::seconds(seconds))
}

#[test]
fn pushback_label_is_wrong_when_nothing_happened_after_the_push() {
    let pushed = Utc::now();
    let idle = StopFacts {
        last_change_at: after(pushed, -30),
        last_commit_at: after(pushed, -20),
        last_report_at: after(pushed, -10),
        ..facts(0, false, false)
    };
    assert_eq!(pushback_label(&idle, pushed), "wrong");
}

#[test]
fn pushback_label_is_right_for_a_change_a_commit_a_report_or_a_check_after_the_push() {
    let pushed = Utc::now();
    let changed = StopFacts {
        last_change_at: after(pushed, 3),
        ..facts(0, false, false)
    };
    let committed = StopFacts {
        last_commit_at: after(pushed, 3),
        ..facts(0, false, false)
    };
    let reported = StopFacts {
        last_report_at: after(pushed, 3),
        ..facts(0, false, false)
    };
    let checked = StopFacts {
        last_check: Some(crate::launch::fleet::decisions::stop_facts::LastCheck {
            passed: false,
            finished_at: after(pushed, 3),
        }),
        ..facts(0, false, false)
    };
    for facts in [changed, committed, reported, checked] {
        assert_eq!(pushback_label(&facts, pushed), "right");
    }
}
