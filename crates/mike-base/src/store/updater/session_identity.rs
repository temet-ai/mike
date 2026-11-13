use std::collections::HashSet;

use uuid::Uuid;

use crate::store::models::{ReminderSource, SystemReminder};
use crate::types::ToolCall;
use crate::wire::steering_block::build_steering_block;

struct SelfIdMismatch<'a> {
    tool_name: &'a str,
    tool_id: &'a str,
    field: &'static str,
    claimed: Uuid,
}

pub fn detect_reminders(tool_calls: &[ToolCall], session_id: &str) -> Vec<SystemReminder> {
    let Ok(session_uuid) = session_id.parse::<Uuid>() else {
        return Vec::new();
    };

    let mut seen_tool_ids: HashSet<&str> = HashSet::new();
    tool_calls
        .iter()
        .filter(|tc| tc.complete)
        .filter(|tc| seen_tool_ids.insert(tc.tool_id.as_str()))
        .filter_map(|tc| find_mismatch(tc, session_uuid))
        .map(|mismatch| build_reminder(&mismatch, session_id))
        .collect()
}

fn find_mismatch(tc: &ToolCall, session_uuid: Uuid) -> Option<SelfIdMismatch<'_>> {
    let field = offending_field(&tc.name)?;
    let input: serde_json::Value = serde_json::from_str(&tc.input_json).ok()?;
    let claimed_raw = input.get(field)?.as_str()?;
    let claimed: Uuid = claimed_raw.parse().ok()?;
    if claimed == session_uuid {
        return None;
    }
    Some(SelfIdMismatch {
        tool_name: &tc.name,
        tool_id: &tc.tool_id,
        field,
        claimed,
    })
}

fn offending_field(tool_name: &str) -> Option<&'static str> {
    if matches_tool(tool_name, "send_session_message") {
        Some("from")
    } else if is_own_session_arg_tool(tool_name) {
        Some("session_id")
    } else {
        None
    }
}

fn is_own_session_arg_tool(tool_name: &str) -> bool {
    ["ack_session_message", "workspace_heartbeat"]
        .iter()
        .any(|base| matches_tool(tool_name, base))
}

fn matches_tool(tool_name: &str, base: &str) -> bool {
    tool_name == base || tool_name.ends_with(&format!("__{base}"))
}

fn build_reminder(mismatch: &SelfIdMismatch<'_>, session_id: &str) -> SystemReminder {
    let body = mismatch_body(mismatch, session_id);
    SystemReminder {
        source: ReminderSource::Diagnostic,
        payload: build_steering_block(&body),
    }
}

fn mismatch_body(mismatch: &SelfIdMismatch<'_>, session_id: &str) -> String {
    let SelfIdMismatch {
        tool_name,
        tool_id,
        field,
        claimed,
    } = mismatch;
    if *field == "from" {
        format!(
            "Your {tool_name} call (tool_use {tool_id}) set from={claimed}, but you are session {session_id}. \
             If that message was sent rather than refused, replies to it will go to {claimed}, not you: \
             resend with request_reply: true (or from={session_id}) and tell the recipient your correct id."
        )
    } else {
        format!(
            "Your {tool_name} call (tool_use {tool_id}) set {field}={claimed}, but you are session {session_id}. \
             That argument must be your own session id. Resend with {field}={session_id}."
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool_call(name: &str, tool_id: &str, input_json: &str) -> ToolCall {
        ToolCall {
            tool_id: tool_id.to_string(),
            name: name.to_string(),
            input_json: input_json.to_string(),
            complete: true,
        }
    }

    #[test]
    fn mismatched_from_produces_one_reminder_naming_both_ids() {
        let session_id = "6f544a61-0000-0000-0000-000000000001";
        let wrong = "da5acf75-0000-0000-0000-000000000002";
        let calls = vec![tool_call(
            "mcp__mike__send_session_message",
            "tool_use_1",
            &format!(r#"{{"from":"{wrong}","to":"peer"}}"#),
        )];
        let reminders = detect_reminders(&calls, session_id);
        assert_eq!(reminders.len(), 1);
        assert!(matches!(reminders[0].source, ReminderSource::Diagnostic));
        assert!(reminders[0].payload.contains(session_id));
        assert!(reminders[0].payload.contains(wrong));
        assert!(reminders[0].payload.contains("tool_use_1"));
    }

    #[test]
    fn matching_from_produces_no_reminder() {
        let session_id = "6f544a61-0000-0000-0000-000000000001";
        let calls = vec![tool_call(
            "mcp__mike__send_session_message",
            "tool_use_1",
            &format!(r#"{{"from":"{session_id}","to":"peer"}}"#),
        )];
        assert!(detect_reminders(&calls, session_id).is_empty());
    }

    #[test]
    fn absent_from_field_produces_no_reminder() {
        let session_id = "6f544a61-0000-0000-0000-000000000001";
        let calls = vec![tool_call(
            "mcp__mike__send_session_message",
            "tool_use_1",
            r#"{"to":"peer","request_reply":true}"#,
        )];
        assert!(detect_reminders(&calls, session_id).is_empty());
    }

    #[test]
    fn non_uuid_from_produces_no_reminder() {
        let session_id = "6f544a61-0000-0000-0000-000000000001";
        let calls = vec![tool_call(
            "mcp__mike__send_session_message",
            "tool_use_1",
            r#"{"from":"my-temp-dir-id","to":"peer"}"#,
        )];
        assert!(detect_reminders(&calls, session_id).is_empty());
    }

    #[test]
    fn same_tool_use_seen_twice_produces_one_reminder() {
        let session_id = "6f544a61-0000-0000-0000-000000000001";
        let wrong = "da5acf75-0000-0000-0000-000000000002";
        let call = tool_call(
            "mcp__mike__send_session_message",
            "tool_use_1",
            &format!(r#"{{"from":"{wrong}","to":"peer"}}"#),
        );
        let calls = vec![call.clone(), call];
        assert_eq!(detect_reminders(&calls, session_id).len(), 1);
    }

    #[test]
    fn own_session_arg_mismatch_on_ack_session_message_is_detected() {
        let session_id = "6f544a61-0000-0000-0000-000000000001";
        let wrong = "da5acf75-0000-0000-0000-000000000002";
        let calls = vec![tool_call(
            "mcp__mike__ack_session_message",
            "tool_use_2",
            &format!(r#"{{"session_id":"{wrong}"}}"#),
        )];
        let reminders = detect_reminders(&calls, session_id);
        assert_eq!(reminders.len(), 1);
        assert!(reminders[0].payload.contains(wrong));
    }

    #[test]
    fn reading_a_peers_inbox_is_not_a_wrong_session_id() {
        let session_id = "6f544a61-0000-0000-0000-000000000001";
        let peer = "da5acf75-0000-0000-0000-000000000002";
        let calls = vec![tool_call(
            "mcp__mike__get_session_messages",
            "tool_use_2",
            &format!(r#"{{"session_id":"{peer}"}}"#),
        )];
        assert!(detect_reminders(&calls, session_id).is_empty());
    }

    #[test]
    fn unrelated_tool_produces_no_reminder() {
        let session_id = "6f544a61-0000-0000-0000-000000000001";
        let calls = vec![tool_call(
            "mcp__mike__list_sessions",
            "tool_use_3",
            r#"{"limit":10}"#,
        )];
        assert!(detect_reminders(&calls, session_id).is_empty());
    }

    #[test]
    fn incomplete_tool_call_is_ignored() {
        let session_id = "6f544a61-0000-0000-0000-000000000001";
        let wrong = "da5acf75-0000-0000-0000-000000000002";
        let mut call = tool_call(
            "mcp__mike__send_session_message",
            "tool_use_4",
            &format!(r#"{{"from":"{wrong}"}}"#),
        );
        call.complete = false;
        assert!(detect_reminders(&[call], session_id).is_empty());
    }

    #[test]
    fn non_uuid_session_id_short_circuits_with_no_reminder() {
        let calls = vec![tool_call(
            "mcp__mike__send_session_message",
            "tool_use_5",
            r#"{"from":"da5acf75-0000-0000-0000-000000000002"}"#,
        )];
        assert!(detect_reminders(&calls, "not-a-uuid").is_empty());
    }
}
