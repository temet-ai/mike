use serde_json::json;

use super::parse::{
    WorkKind, notifications_in_request_body, parse_launch, parse_notifications, parse_resume,
    parse_stop, result_text,
};

#[test]
fn task_stop_result_finishes_the_task_as_killed() {
    let text = "{\"message\":\"Successfully stopped task: bvvzqrjuy (docker exec mike-test-pg psql)\",\"task_id\":\"bvvzqrjuy\"}";
    let note = parse_stop("TaskStop", text);
    assert_eq!(note.as_ref().map(|n| n.task_id.as_str()), Some("bvvzqrjuy"));
    assert_eq!(note.map(|n| n.state()), Some("killed"));
    assert!(parse_stop("Bash", text).is_none());
    assert!(parse_stop("TaskStop", "No task found with ID: bvvzqrjuy").is_none());
}

const BASH_RESULT: &str = "Command running in background with ID: bvg0che6p. Output is being written to: /tmp/claude-1000/-mnt-data-projects-temet-temet-proxy/da5acf75-a26b-4f3d-9d75-c382a8e81985/tasks/bvg0che6p.output. You will be notified when it completes. To check interim output, use Read on that file path.";

const AGENT_RESULT: &str = "Async agent launched successfully. (This tool result is internal metadata — never quote or paste any part of it, including the agentId below, into a user-facing reply.)\nagentId: a3f26dd181b8925f3 (internal ID - do not mention to user. Use SendMessage with to: 'a3f26dd181b8925f3', summary: '<5-10 word recap>' to continue this agent.)\nThe agent is working in the background.\noutput_file: /tmp/claude-1000/x/tasks/a3f26dd181b8925f3.output\nDo NOT Read or tail this file";

const NOTE_BASH: &str = "<task-notification>\n<task-id>byz3urejj</task-id>\n<tool-use-id>toolu_01F1YSzddLqkSLfxwkd5mvGw</tool-use-id>\n<output-file>/tmp/claude-1000/x/tasks/byz3urejj.output</output-file>\n<status>completed</status>\n<summary>Background command \"Warm up P4\" completed (exit code 0)</summary>\n</task-notification>";

const NOTE_AGENT_KILLED: &str = "<task-notification>\n<task-id>a6fecf0fcf0e7b063</task-id>\n<tool-use-id>toolu_01MPRbq3X5vYn9oMYe4LoLz7</tool-use-id>\n<status>killed</status>\n<summary>Agent \"Workbench\" was stopped by Claude</summary>\n<note>A task-notification fires each time</note>\n<result>I'll wait for this background build to finish before continuing.</result>\n</task-notification>";

#[test]
fn bash_background_launch_yields_task_id_and_output_path() -> anyhow::Result<()> {
    let launch = parse_launch("Bash", BASH_RESULT).ok_or_else(|| anyhow::anyhow!("no launch"))?;
    assert_eq!(launch.kind, WorkKind::Bash);
    assert_eq!(launch.task_id, "bvg0che6p");
    assert_eq!(
        launch.output_path.as_deref(),
        Some(
            "/tmp/claude-1000/-mnt-data-projects-temet-temet-proxy/da5acf75-a26b-4f3d-9d75-c382a8e81985/tasks/bvg0che6p.output"
        )
    );
    Ok(())
}

#[test]
fn agent_launch_yields_agent_id_and_output_file() -> anyhow::Result<()> {
    let launch = parse_launch("Agent", AGENT_RESULT).ok_or_else(|| anyhow::anyhow!("no launch"))?;
    assert_eq!(launch.kind, WorkKind::Agent);
    assert_eq!(launch.task_id, "a3f26dd181b8925f3");
    assert_eq!(
        launch.output_path.as_deref(),
        Some("/tmp/claude-1000/x/tasks/a3f26dd181b8925f3.output")
    );
    Ok(())
}

#[test]
fn launch_phrase_quoted_by_another_tool_is_not_a_launch() {
    assert!(parse_launch("Read", BASH_RESULT).is_none());
    let quoted = format!("117 artifacts/spec.md\n{BASH_RESULT}");
    assert!(parse_launch("Bash", &quoted).is_none());
}

#[test]
fn foreground_bash_output_is_not_a_launch() {
    assert!(parse_launch("Bash", "ok\n").is_none());
}

#[test]
fn notification_with_exit_code_zero_is_completed() {
    let notes = parse_notifications(NOTE_BASH);
    assert_eq!(notes.len(), 1);
    let note = &notes[0];
    assert_eq!(note.task_id, "byz3urejj");
    assert_eq!(
        note.tool_use_id.as_deref(),
        Some("toolu_01F1YSzddLqkSLfxwkd5mvGw")
    );
    assert_eq!(note.exit_code, Some(0));
    assert_eq!(note.state(), "completed");
}

#[test]
fn nonzero_exit_code_maps_to_failed() {
    let note = parse_notifications(&NOTE_BASH.replace("(exit code 0)", "(exit code 144)"));
    assert_eq!(note[0].exit_code, Some(144));
    assert_eq!(note[0].state(), "failed");
}

#[test]
fn killed_agent_maps_to_killed() {
    let notes = parse_notifications(NOTE_AGENT_KILLED);
    assert_eq!(notes[0].task_id, "a6fecf0fcf0e7b063");
    assert_eq!(notes[0].state(), "killed");
}

#[test]
fn several_notifications_in_one_text_are_all_found() {
    let text = format!("{NOTE_BASH}\nsome text\n{NOTE_AGENT_KILLED}");
    let ids: Vec<_> = parse_notifications(&text)
        .into_iter()
        .map(|n| n.task_id)
        .collect();
    assert_eq!(ids, vec!["byz3urejj", "a6fecf0fcf0e7b063"]);
}

#[test]
fn unterminated_notification_is_ignored() {
    assert!(parse_notifications("<task-notification><task-id>x</task-id>").is_empty());
}

#[test]
fn request_body_notifications_come_from_the_last_message_only() -> anyhow::Result<()> {
    let body = json!({
        "messages": [
            {"role": "user", "content": NOTE_AGENT_KILLED},
            {"role": "assistant", "content": [{"type": "text", "text": "ok"}]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": [{"type": "text", "text": "ok"}]},
                {"type": "text", "text": NOTE_BASH}
            ]}
        ]
    });
    let bytes = serde_json::to_vec(&body)?;
    let ids: Vec<_> = notifications_in_request_body(&bytes)
        .into_iter()
        .map(|n| n.task_id)
        .collect();
    assert_eq!(ids, vec!["byz3urejj"]);
    Ok(())
}

#[test]
fn request_body_without_notifications_is_cheap_and_empty() {
    assert!(
        notifications_in_request_body(br#"{"messages":[{"role":"user","content":"hi"}]}"#)
            .is_empty()
    );
    assert!(notifications_in_request_body(b"not json task-notification>").is_empty());
}

#[test]
fn result_text_joins_text_blocks() {
    let content =
        json!([{"type": "text", "text": "a"}, {"type": "image"}, {"type": "text", "text": "b"}]);
    assert_eq!(result_text(&content), "a\nb");
    assert_eq!(result_text(&json!("plain")), "plain");
}

#[test]
fn send_message_resume_yields_the_resumed_agent_id() {
    let text = r#"{"success":true,"message":"Resuming agent aafe107","resumedAgentId":"aafe107b9b0b5a67c","pin":{"id":"aafe107b9b0b5a67c"}}"#;
    assert_eq!(
        parse_resume("SendMessage", text).as_deref(),
        Some("aafe107b9b0b5a67c")
    );
    assert!(parse_resume("Bash", text).is_none());
    assert!(parse_resume("SendMessage", r#"{"success":true,"message":"queued"}"#).is_none());
}

#[test]
fn notifications_quoted_inside_tool_results_are_ignored() -> anyhow::Result<()> {
    let body = json!({
        "messages": [
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t9", "content": [{"type": "text", "text": NOTE_AGENT_KILLED}]},
                {"type": "text", "text": NOTE_BASH}
            ]}
        ]
    });
    let bytes = serde_json::to_vec(&body)?;
    let ids: Vec<_> = notifications_in_request_body(&bytes)
        .into_iter()
        .map(|n| n.task_id)
        .collect();
    assert_eq!(ids, vec!["byz3urejj"]);
    Ok(())
}
