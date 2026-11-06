use super::*;

#[test]
fn response_trigger_gate_turn_end_skipped_on_tool_use_stop() {
    assert!(!response_trigger_gate("turn_end", Some("tool_use"), true));
}

#[test]
fn response_trigger_gate_turn_end_evaluated_on_end_turn_with_text() {
    assert!(response_trigger_gate("turn_end", Some("end_turn"), true));
}

#[test]
fn response_trigger_gate_turn_end_skipped_on_end_turn_with_empty_text() {
    assert!(!response_trigger_gate("turn_end", Some("end_turn"), false));
}

#[test]
fn response_trigger_gate_turn_end_skipped_when_stop_reason_missing() {
    assert!(!response_trigger_gate("turn_end", None, true));
}

#[test]
fn response_trigger_gate_response_steerers_unaffected_by_stop_reason() {
    for stop_reason in [None, Some("tool_use"), Some("end_turn"), Some("refusal")] {
        assert!(response_trigger_gate("response", stop_reason, true));
        assert!(response_trigger_gate("response", stop_reason, false));
        assert!(response_trigger_gate("", stop_reason, true));
    }
}

#[test]
fn is_work_lane_allows_only_named_work_origins() {
    for lane in ["primary", "primary_first"] {
        assert!(is_work_lane(lane), "{lane} should be a work lane");
    }
}

#[test]
fn is_work_lane_rejects_sidecar_and_probe_origins() {
    for lane in [
        "title_sidecar",
        "monitor_sidecar",
        "advisor_sidecar",
        "count_tokens_sidecar",
        "web_fetch_sidecar",
        "next_input_suggestion_sidecar",
        "quota_probe",
        "subagent",
        "workflow_subagent",
        "",
        "some_future_sidecar",
    ] {
        assert!(!is_work_lane(lane), "{lane} should not be a work lane");
    }
}
