use chrono::{Duration, Utc};

use super::*;
use crate::store::persistence::mailbox::MailboxMessageRow;

fn base_facts() -> StopFacts {
    StopFacts {
        seat: "worker-T1".into(),
        task: "T1".into(),
        uncommitted: Vec::new(),
        last_change_at: None,
        check_passed_after_change: false,
        last_check: None,
        open_asks: Vec::new(),
        check_running: false,
        done_when: None,
        last_commit_sha: None,
        last_commit_at: None,
        last_report_at: None,
        report_filed_awaiting_verdict: false,
        last_report_refusal: None,
    }
}

fn check(passed: bool, offset_seconds: i64) -> LastCheck {
    LastCheck {
        passed,
        finished_at: Some(Utc::now() + Duration::seconds(offset_seconds)),
    }
}

#[test]
fn passed_after_a_passing_check_newer_than_the_change_is_true() {
    let change = Utc::now();
    assert!(passed_after(Some(&check(true, 5)), Some(change)));
}

#[test]
fn passed_after_a_passing_check_older_than_the_change_is_false() {
    let change = Utc::now();
    assert!(!passed_after(Some(&check(true, -60)), Some(change)));
}

#[test]
fn passed_after_a_failing_check_is_false() {
    assert!(!passed_after(Some(&check(false, 5)), Some(Utc::now())));
}

#[test]
fn passed_after_no_check_is_false() {
    assert!(!passed_after(None, Some(Utc::now())));
}

#[test]
fn unchecked_changes_needs_uncommitted_files_and_no_passing_check() {
    let mut facts = StopFacts {
        seat: "worker-T1".into(),
        task: "T1".into(),
        uncommitted: vec!["src/a.rs".into()],
        last_change_at: None,
        check_passed_after_change: false,
        last_check: None,
        open_asks: Vec::new(),
        check_running: false,
        done_when: None,
        last_commit_sha: None,
        last_commit_at: None,
        last_report_at: None,
        report_filed_awaiting_verdict: false,
        last_report_refusal: None,
    };
    assert!(facts.unchecked_changes());
    facts.check_passed_after_change = true;
    assert!(!facts.unchecked_changes());
    facts.check_passed_after_change = false;
    facts.uncommitted.clear();
    assert!(!facts.unchecked_changes());
}

fn event(id: i64, kind: EventKind, at: i64, payload: Value) -> FleetEvent {
    FleetEvent {
        id,
        run_id: "run".into(),
        task: Some("T1".into()),
        session_uuid: None,
        kind,
        payload,
        occurred_at: Utc::now() + Duration::seconds(at),
    }
}

#[test]
fn task_record_reads_the_newest_commit() {
    let events = [
        event(1, EventKind::CommitLanded, -50, json!({"sha": "aaa"})),
        event(2, EventKind::CommitLanded, -10, json!({"sha": "bbb"})),
    ];
    let record = task_record(&events);
    assert_eq!(record.last_commit_sha.as_deref(), Some("bbb"));
    assert_eq!(record.last_commit_at, Some(events[1].occurred_at));
    assert!(!record.report_awaiting_verdict);
}

#[test]
fn task_record_a_report_without_a_verdict_awaits_one() {
    let note = Uuid::new_v4();
    let events = [event(
        1,
        EventKind::ReportFiled,
        -5,
        json!({"note_uuid": note}),
    )];
    let record = task_record(&events);
    assert!(record.report_awaiting_verdict);
    assert_eq!(record.last_report_at, Some(events[0].occurred_at));
}

#[test]
fn task_record_a_verdict_on_the_report_settles_it() {
    let note = Uuid::new_v4();
    let events = [
        event(1, EventKind::ReportFiled, -9, json!({"note_uuid": note})),
        event(
            2,
            EventKind::VerdictRecorded,
            -5,
            json!({"note_uuid": note, "criterion": "c", "passed": false}),
        ),
    ];
    assert!(!task_record(&events).report_awaiting_verdict);
}

#[test]
fn task_record_a_later_report_awaits_its_own_verdict() {
    let (first, second) = (Uuid::new_v4(), Uuid::new_v4());
    let events = [
        event(1, EventKind::ReportFiled, -20, json!({"note_uuid": first})),
        event(
            2,
            EventKind::VerdictRecorded,
            -15,
            json!({"note_uuid": first, "criterion": "c", "passed": false}),
        ),
        event(3, EventKind::ReportFiled, -5, json!({"note_uuid": second})),
    ];
    assert!(task_record(&events).report_awaiting_verdict);
}

fn mail(
    (from, to): (Uuid, Uuid),
    body: &str,
    queued_offset: i64,
    in_reply_to: Option<Uuid>,
) -> MailboxMessageWithActivity {
    MailboxMessageWithActivity {
        row: MailboxMessageRow {
            message_uuid: Uuid::new_v4(),
            to_session_uuid: to,
            from_session_uuid: Some(from),
            body: body.into(),
            queued_at: Utc::now() + Duration::seconds(queued_offset),
            claimed_at: None,
            sent_request_id: None,
            sent_at: None,
            consumed_at: None,
            dead_lettered_at: None,
            channel_delivered_at: None,
            delivery_attempts: 0,
            thread_uuid: None,
            in_reply_to_uuid: in_reply_to,
        },
        recipient_last_request_at: None,
        recipient_last_eligible_turn_at: None,
        context_items: Vec::new(),
    }
}

#[test]
fn unanswered_mail_lists_mail_to_the_coordinator_with_no_reply() {
    let (seat, coordinator) = (Uuid::new_v4(), Uuid::new_v4());
    let sent = mail((seat, coordinator), "may I stop?", -30, None);
    let open = unanswered_mail((&[sent], &[]), &[coordinator]);
    assert_eq!(open.len(), 1);
    assert!(open[0].contains("may I stop?"));
}

#[test]
fn unanswered_mail_a_later_message_from_the_recipient_answers_it() {
    let (seat, coordinator) = (Uuid::new_v4(), Uuid::new_v4());
    let sent = mail((seat, coordinator), "may I stop?", -30, None);
    let reply = mail((coordinator, seat), "yes", -10, None);
    assert!(unanswered_mail((&[sent], &[reply]), &[coordinator]).is_empty());
}

#[test]
fn unanswered_mail_a_reply_that_names_the_message_answers_it() {
    let (seat, coordinator) = (Uuid::new_v4(), Uuid::new_v4());
    let sent = mail((seat, coordinator), "may I stop?", -30, None);
    let reply = mail((coordinator, seat), "yes", -40, Some(sent.row.message_uuid));
    assert!(unanswered_mail((&[sent], &[reply]), &[coordinator]).is_empty());
}

#[test]
fn unanswered_mail_ignores_mail_to_other_seats() {
    let (seat, coordinator, peer) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let sent = mail((seat, peer), "fyi", -30, None);
    assert!(unanswered_mail((&[sent], &[]), &[coordinator]).is_empty());
}

fn check_row(finished: bool, started_minutes_ago: i64) -> TreeCheckRow {
    let started_at = Utc::now() - Duration::minutes(started_minutes_ago);
    TreeCheckRow {
        id: 1,
        run_id: "run".into(),
        state: "s".into(),
        state_after: None,
        command: "check".into(),
        started_at,
        finished_at: finished.then_some(started_at),
        exit_code: None,
        output: String::new(),
        askers: Vec::new(),
        told: Vec::new(),
    }
}

#[test]
fn check_pending_a_queued_check_counts_as_running() {
    assert!(check_pending(Some(&check_row(false, 2)), Utc::now()));
}

#[test]
fn check_pending_a_finished_or_long_abandoned_check_does_not() {
    assert!(!check_pending(Some(&check_row(true, 2)), Utc::now()));
    assert!(!check_pending(Some(&check_row(false, 120)), Utc::now()));
    assert!(!check_pending(None, Utc::now()));
}

#[test]
fn report_refusal_shows_until_a_newer_report_is_filed() {
    let session = Uuid::new_v4();
    note_report_refusal(session, "slice names no criterion");
    assert_eq!(
        refusal_after(session, None).as_deref(),
        Some("slice names no criterion")
    );
    let newer = Utc::now() + Duration::seconds(5);
    assert_eq!(refusal_after(session, Some(newer)), None);
    assert_eq!(refusal_after(Uuid::new_v4(), None), None);
}

#[test]
fn state_carries_the_report_and_commit_facts_for_the_model() {
    let facts = StopFacts {
        last_commit_sha: Some("abc123".into()),
        report_filed_awaiting_verdict: true,
        last_report_refusal: Some("refused".into()),
        ..base_facts()
    };
    let state = facts.state("done");
    assert_eq!(state["last_commit_sha"], "abc123");
    assert_eq!(state["a_report_is_filed_and_awaits_its_verdict"], true);
    assert_eq!(state["last_report_refusal"], "refused");
}

#[test]
fn worked_since_needs_an_event_after_the_given_time() {
    let since = Utc::now();
    let quiet = base_facts();
    assert!(!quiet.worked_since(since));
    let committed = StopFacts {
        last_commit_at: Some(since + Duration::seconds(1)),
        ..base_facts()
    };
    assert!(committed.worked_since(since));
}
