use super::call::{ReplyRequest, label_for, reply_input, reply_schema};
use super::{ReplyVerdict, from_verdict, reply_text_from, undelivered};
use crate::store::persistence::dossier_view::OpenTextRow;
use crate::store::persistence::steering::ReplyLabelCandidate;

fn candidate(delivered: Option<i64>, reverted: bool) -> ReplyLabelCandidate {
    ReplyLabelCandidate {
        uuid: uuid::Uuid::from_u128(7),
        steerer_id: 15,
        steerer_name: "monitor-over-sleep".to_owned(),
        steerer_rule: None,
        session_id: 1,
        operator_turn_id: 2,
        question: None,
        payload_text: Some("Use the Monitor tool.".to_owned()),
        trigger_request_id: Some(10),
        delivered_request_id: delivered,
        reply_turn_id: delivered.map(|_| 3),
        reverted,
        revert_reason: reverted.then(|| "superseded_by_operator".to_owned()),
    }
}

#[test]
fn only_a_yes_is_right_and_only_a_no_is_wrong() {
    assert_eq!(label_for("yes"), "right");
    assert_eq!(label_for("no"), "wrong");
    assert_eq!(label_for("partly"), "indeterminate");
    assert_eq!(label_for("unclear"), "indeterminate");
}

#[test]
fn a_verdict_keeps_the_reaction_and_quote_as_evidence() {
    let v = ReplyVerdict {
        note_correct: "no".to_owned(),
        reaction: "declined".to_owned(),
        failure: "short_pause".to_owned(),
        quote: "a one-off 8 s check".to_owned(),
    };
    let label = from_verdict(&candidate(Some(11), false), &v, "m");
    assert_eq!(label.label, "wrong");
    assert_eq!(label.source, "session_reply");
    assert_eq!(label.evidence["reaction"], "declined");
    assert_eq!(label.evidence["failure"], "short_pause");
    assert_eq!(label.evidence["delivered_request_id"], 11);
}

#[test]
fn a_note_that_never_reached_the_session_is_indeterminate() {
    let label = undelivered(&candidate(None, true));
    assert_eq!(label.label, "indeterminate");
    assert_eq!(label.evidence["reaction"], "no_reply");
    assert_eq!(label.evidence["undelivered"], "reverted");
    assert_eq!(
        undelivered(&candidate(None, false)).evidence["undelivered"],
        "expired"
    );
}

#[test]
fn the_input_shows_note_turn_end_and_reply() {
    let long_turn = format!("{}THE END", "x".repeat(5_000));
    let text = reply_input(&ReplyRequest {
        steerer: "verifier",
        rule: "Fire only on a settled done claim.",
        note: "You claimed done.",
        turn_text: &long_turn,
        turn_tools: &[],
        reply_text: "It doesn't apply.",
        reply_tools: &[],
    });
    assert!(text.contains("You claimed done."));
    assert!(text.contains("Fire only on a settled done claim."));
    assert!(text.contains("THE END"));
    assert!(text.contains("It doesn't apply."));
    assert!(text.contains("(no tool calls)"));
    assert!(text.chars().count() < 4_000);
}

#[test]
fn the_schema_requires_every_field() {
    let schema = reply_schema();
    assert_eq!(schema["required"].as_array().map(Vec::len), Some(4));
}

#[test]
fn a_steerer_is_switched_off_only_when_its_notes_are_clearly_mostly_wrong() {
    use super::demotion::should_switch_off;
    assert!(!should_switch_off(1, 5));
    assert!(should_switch_off(1, 9));
    assert!(!should_switch_off(4, 6));
    assert!(!should_switch_off(20, 10));
    assert!(should_switch_off(10, 30));
}

fn text_row(request_id: i64, content: &str) -> OpenTextRow {
    OpenTextRow {
        request_id,
        seq_ord: 0,
        origin: "primary".to_owned(),
        content: content.to_owned(),
    }
}

#[test]
fn the_reply_is_everything_the_session_wrote_from_the_note_to_the_end_of_the_turn() {
    let rows = [
        text_row(9, "work before the note arrived"),
        text_row(11, "  "),
        text_row(12, "Checking the settings file first."),
        text_row(15, "That note does not apply: it was done earlier."),
    ];
    assert_eq!(
        reply_text_from(&rows, 11),
        "Checking the settings file first.\n\nThat note does not apply: it was done earlier."
    );
    assert_eq!(reply_text_from(&rows, 99), "");
}

#[test]
fn a_long_reply_is_shown_by_its_start_and_its_end() {
    let long_reply = format!("FIRST WORDS {} LAST WORDS", "y".repeat(6_000));
    let text = reply_input(&ReplyRequest {
        steerer: "verifier",
        rule: "",
        note: "You claimed done.",
        turn_text: "done",
        turn_tools: &[],
        reply_text: &long_reply,
        reply_tools: &[],
    });
    assert!(text.contains("## SESSION'S REPLY (start)\nFIRST WORDS"));
    assert!(text.contains("LAST WORDS\n"));
    assert!(text.contains("## SESSION'S REPLY (end)"));
    assert!(text.chars().count() < 4_500);
}
