use super::*;

fn request(turn_type: &str, author: &str, origin: &str, text: &str) -> SeatRequest {
    SeatRequest {
        request_id: 1,
        seat: "worker-T1".into(),
        session_id: 7,
        turn_type: turn_type.into(),
        author_kind: author.into(),
        turn_origin: origin.into(),
        text: text.into(),
    }
}

#[test]
fn request_trigger_a_tool_loop_turn_is_a_tool_result() {
    let row = request("tool_loop", "agent", "primary", "");
    assert_eq!(request_trigger(&row), Trigger::ToolResult);
}

#[test]
fn request_trigger_a_stop_hook_prompt_is_a_hook() {
    let row = request(
        "user_prompt",
        "harness",
        "primary",
        "Stop hook feedback: keep going",
    );
    assert_eq!(request_trigger(&row), Trigger::Hook);
}

#[test]
fn request_trigger_a_mail_envelope_is_mail() {
    let row = request(
        "user_prompt",
        "harness",
        "primary",
        "<channel source=\"mike\">hi</channel>",
    );
    assert_eq!(request_trigger(&row), Trigger::Mail);
}

#[test]
fn request_trigger_the_silent_seat_reminder_is_a_wakeup() {
    let text = "You have 2 unread mike message(s) and have been silent for five minutes.";
    let row = request("user_prompt", "harness", "primary", text);
    assert_eq!(request_trigger(&row), Trigger::Wakeup);
}

#[test]
fn request_trigger_a_steered_turn_is_steering() {
    let row = request("user_prompt", "harness", "steered", "check your work");
    assert_eq!(request_trigger(&row), Trigger::Steering);
}

#[test]
fn request_trigger_a_typed_prompt_is_the_operator() {
    let row = request("user_prompt", "operator", "primary", "please start");
    assert_eq!(request_trigger(&row), Trigger::Operator);
}

#[test]
fn request_trigger_a_sidecar_is_a_hook() {
    let row = request("user_prompt", "harness", "title_sidecar", "name this chat");
    assert_eq!(request_trigger(&row), Trigger::Hook);
}

#[test]
fn trigger_label_hooks_and_steering_are_machinery_and_the_rest_are_left_to_content() {
    assert_eq!(trigger_label(Trigger::Hook), Some(Label::Machinery));
    assert_eq!(trigger_label(Trigger::Steering), Some(Label::Machinery));
    for trigger in [
        Trigger::Operator,
        Trigger::Mail,
        Trigger::Wakeup,
        Trigger::ToolResult,
    ] {
        assert_eq!(trigger_label(trigger), None);
    }
}

#[test]
fn effort_row_records_who_decided_the_label() {
    let hook = effort_row(&request(
        "user_prompt",
        "harness",
        "primary",
        "Stop hook feedback: x",
    ));
    assert_eq!(hook.label.as_deref(), Some("machinery"));
    assert_eq!(hook.decided_by.as_deref(), Some("trigger"));
    let tool = effort_row(&request("tool_loop", "agent", "primary", ""));
    assert_eq!(tool.label, None);
}

fn share(seat: &str, label: &str, tokens: i64) -> EffortShare {
    EffortShare {
        seat: seat.into(),
        label: label.into(),
        requests: 1,
        tokens,
    }
}

#[test]
fn shares_document_adds_percentages_by_tokens_per_seat_and_overall() {
    let rows = vec![
        share("a", "work", 300),
        share("a", "machinery", 100),
        share("b", "waiting", 100),
    ];
    let doc = shares_document(&rows);
    assert_eq!(doc["by_seat"]["a"]["labels"]["work"]["share_percent"], 75.0);
    assert_eq!(
        doc["by_seat"]["a"]["labels"]["machinery"]["share_percent"],
        25.0
    );
    assert_eq!(doc["all_seats"]["tokens"], 500);
    assert_eq!(doc["all_seats"]["labels"]["work"]["share_percent"], 60.0);
}

#[test]
fn shares_document_an_empty_run_has_no_shares() {
    let doc = shares_document(&[]);
    assert_eq!(doc["all_seats"]["tokens"], 0);
}

fn step(id: i64) -> Unlabelled {
    Unlabelled {
        request_id: id,
        seat: "a".into(),
        trigger: "tool_result".into(),
        text: "ran cargo test".into(),
        tools: "Bash: cargo test".into(),
    }
}

#[test]
fn compose_reads_one_label_per_step_and_skips_a_failed_call() {
    let steps = vec![step(1), step(2)];
    assert_eq!(questions(&steps).len(), 2);
    assert!(compose(&Asked::failed_with("timeout"), &steps).is_empty());
}
