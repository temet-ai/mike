use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use uuid::Uuid;

use super::super::author::{
    Authorship, explains_the_change, proven_edit, proven_late, ran_at, written_texts,
};
use super::super::refs::one_seat;
use crate::store::persistence::change_record::EditCall;

fn edit(session_id: i64, tool_use_id: i64, at: DateTime<Utc>, new_string: &str) -> EditCall {
    EditCall {
        session_id,
        tool_use_id,
        tool_name: "Edit".into(),
        at: at - Duration::seconds(3),
        ran_from: at - Duration::milliseconds(20),
        ran_to: Some(at + Duration::milliseconds(60)),
        failed: Some(false),
        input: Some(json!({"file_path": "/r/a.py", "old_string": "x", "new_string": new_string})),
    }
}

#[test]
fn an_edit_that_ran_when_the_file_changed_and_whose_text_is_in_it_is_proven() {
    let at = Utc::now();
    let who = proven_edit(&[edit(4, 40, at, "def cc():")], at, "import x\ndef cc():\n");
    assert_eq!(
        who.as_ref()
            .map(|author| author.detail()["content_verified"].clone()),
        Some(serde_json::json!(true))
    );
    assert_eq!(
        who,
        Some(Authorship::Edit {
            session: 4,
            tool_use_id: 40,
            tool_name: "Edit".into(),
            content_verified: true
        })
    );
}

#[test]
fn an_edit_whose_text_is_not_in_the_file_is_not_proven() {
    let at = Utc::now();
    assert_eq!(
        proven_edit(&[edit(4, 40, at, "def cc():")], at, "import x\n"),
        None
    );
}

#[test]
fn an_edit_that_finished_long_before_the_change_is_not_proven() {
    let at = Utc::now();
    let mut call = edit(4, 40, at, "def cc():");
    call.ran_from = at - Duration::seconds(20);
    call.ran_to = Some(at - Duration::seconds(10));
    assert!(!ran_at(&call, at));
    assert_eq!(proven_edit(&[call], at, "def cc():"), None);
}

#[test]
fn an_edit_whose_result_is_not_captured_yet_counts_as_running() {
    let at = Utc::now();
    let mut call = edit(4, 40, at, "def cc():");
    call.ran_to = None;
    assert!(ran_at(&call, at));
    call.ran_from = at + Duration::seconds(5);
    assert!(!ran_at(&call, at));
}

#[test]
fn an_edit_whose_tool_result_is_an_error_is_not_proven() {
    let at = Utc::now();
    let mut call = edit(4, 40, at, "def cc():");
    call.failed = Some(true);
    assert!(!ran_at(&call, at));
    assert_eq!(proven_edit(&[call], at, "def cc():"), None);
}

#[test]
fn two_sessions_both_proven_name_no_author() {
    let at = Utc::now();
    let calls = [edit(4, 40, at, "one"), edit(5, 50, at, "two")];
    assert_eq!(proven_edit(&calls, at, "one two"), None);
}

#[test]
fn written_texts_reads_edit_write_and_multiedit_and_refuses_an_empty_text() {
    let at = Utc::now();
    let mut call = edit(1, 1, at, "new");
    assert_eq!(written_texts(&call), Some(vec!["new"]));
    call.input = Some(json!({"new_string": ""}));
    assert_eq!(written_texts(&call), None);
    call.tool_name = "Write".into();
    call.input = Some(json!({"content": "whole file"}));
    assert_eq!(written_texts(&call), Some(vec!["whole file"]));
    call.tool_name = "MultiEdit".into();
    call.input = Some(json!({"edits": [{"new_string": "a"}, {"new_string": "b"}]}));
    assert_eq!(written_texts(&call), Some(vec!["a", "b"]));
    call.input = None;
    assert_eq!(written_texts(&call), None);
}

#[test]
fn one_seat_names_a_seat_only_when_every_pushed_commit_carries_it() {
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    assert_eq!(one_seat(&[Some(a), Some(a)]), Some(a));
    assert_eq!(one_seat(&[Some(a), Some(b)]), None);
    assert_eq!(one_seat(&[Some(a), None]), None);
    assert_eq!(one_seat(&[]), None);
}

fn edit_of(old: &str, new: &str) -> EditCall {
    let mut call = edit(4, 40, Utc::now(), new);
    call.input = Some(json!({"old_string": old, "new_string": new}));
    call
}

fn finished_edit(
    session_id: i64,
    tool_use_id: i64,
    ended: DateTime<Utc>,
    old: &str,
    new: &str,
) -> EditCall {
    let mut call = edit(session_id, tool_use_id, ended, new);
    call.ran_from = ended - Duration::milliseconds(80);
    call.ran_to = Some(ended);
    call.at = ended - Duration::seconds(1);
    call.input = Some(json!({"old_string": old, "new_string": new}));
    call
}

#[test]
fn an_edit_seen_thirty_seconds_late_is_named_when_it_alone_explains_the_change() {
    let at = Utc::now();
    let call = finished_edit(4, 40, at - Duration::seconds(30), "one", "ONE");
    assert_eq!(
        proven_late(&[call], at, Some("a\none\n"), "a\nONE\n"),
        Some(Authorship::Edit {
            session: 4,
            tool_use_id: 40,
            tool_name: "Edit".into(),
            content_verified: true
        })
    );
}

#[test]
fn a_late_edit_is_not_named_when_the_file_also_changed_elsewhere() {
    let at = Utc::now();
    let call = finished_edit(4, 40, at - Duration::seconds(30), "one", "ONE");
    assert_eq!(
        proven_late(&[call], at, Some("a\none\n"), "a\nONE\nformatted\n"),
        None
    );
}

#[test]
fn a_late_edit_already_in_the_content_before_names_nobody_for_a_later_change() {
    let at = Utc::now();
    let call = finished_edit(4, 40, at - Duration::seconds(20), "one", "ONE");
    assert_eq!(
        proven_late(&[call], at, Some("a\nONE\n"), "a\nONE\n\n"),
        None
    );
}

#[test]
fn late_edits_by_two_sessions_name_no_author() {
    let at = Utc::now();
    let calls = [
        finished_edit(4, 40, at - Duration::seconds(20), "one", "ONE"),
        finished_edit(5, 50, at - Duration::seconds(10), "two", "TWO"),
    ];
    assert_eq!(proven_late(&calls, at, Some("one two"), "ONE TWO"), None);
}

#[test]
fn of_a_session_s_late_edits_the_newest_that_explain_the_change_name_it() {
    let at = Utc::now();
    let calls = [
        finished_edit(4, 40, at - Duration::seconds(25), "zero", "one"),
        finished_edit(4, 41, at - Duration::seconds(15), "one", "ONE"),
    ];
    assert_eq!(
        proven_late(&calls, at, Some("x one"), "x ONE"),
        Some(Authorship::Edit {
            session: 4,
            tool_use_id: 41,
            tool_name: "Edit".into(),
            content_verified: true
        })
    );
}

#[test]
fn a_late_edit_whose_tool_result_is_an_error_names_nobody() {
    let at = Utc::now();
    let mut call = finished_edit(4, 40, at - Duration::seconds(30), "one", "ONE");
    call.failed = Some(true);
    assert_eq!(proven_late(&[call], at, Some("one"), "ONE"), None);
}

#[test]
fn an_edit_alone_explains_a_change_that_is_exactly_its_replacement() {
    let call = edit_of("one", "ONE");
    assert!(explains_the_change(
        &[&call],
        Some("a\none\nb\n"),
        "a\nONE\nb\n"
    ));
}

#[test]
fn an_edit_does_not_explain_a_change_that_also_altered_other_lines() {
    let call = edit_of("one", "ONE");
    assert!(!explains_the_change(
        &[&call],
        Some("a\none\nb\n"),
        "a\nONE\nb\nextra\n"
    ));
}

#[test]
fn an_edit_cannot_be_checked_without_the_content_before_it_or_when_its_old_text_is_missing() {
    let call = edit_of("one", "ONE");
    assert!(!explains_the_change(&[&call], None, "ONE"));
    assert!(!explains_the_change(&[&call], Some("two"), "ONE"));
}

#[test]
fn a_write_explains_a_change_that_is_exactly_its_content_and_a_multiedit_applies_in_turn() {
    let mut call = edit(4, 40, Utc::now(), "whole");
    call.tool_name = "Write".into();
    call.input = Some(json!({"content": "whole"}));
    assert!(explains_the_change(&[&call], None, "whole"));
    assert!(!explains_the_change(&[&call], None, "whole\n"));
    let mut multi = edit(4, 41, Utc::now(), "x");
    multi.tool_name = "MultiEdit".into();
    multi.input = Some(json!({"edits": [
        {"old_string": "a", "new_string": "b"},
        {"old_string": "b", "new_string": "c"}
    ]}));
    assert!(explains_the_change(&[&multi], Some("a"), "c"));
}
