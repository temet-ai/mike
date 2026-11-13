use std::path::PathBuf;

use super::*;

const SAMPLE_EVENTS: &str = r#"{"type":"thread.started","thread_id":"01a136aa-4c78-78b3-b5d8-414445754858"}
{"type":"turn.started"}
{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"{\"rule\":\"x\"}"}}
{"type":"turn.completed","usage":{"input_tokens":14890,"cached_input_tokens":12,"cache_write_input_tokens":0,"output_tokens":25,"reasoning_output_tokens":3}}
"#;

#[test]
fn reasoning_effort_blank_becomes_none_and_others_pass_through() {
    assert_eq!(codex_reasoning_effort(""), "none");
    assert_eq!(codex_reasoning_effort("none"), "none");
    assert_eq!(codex_reasoning_effort("low"), "low");
}

const FAILED_EVENTS: &str = r#"{"type":"thread.started","thread_id":"t1"}
{"type":"turn.started"}
{"type":"error","message":"{\"error\":{\"message\":\"Unsupported value: 'minimal' is not supported with the 'gpt-6-sol' model.\",\"param\":\"reasoning.effort\"}}"}
{"type":"turn.failed","error":{"message":"{\"error\":{\"message\":\"Unsupported value: 'minimal' is not supported with the 'gpt-6-sol' model.\",\"param\":\"reasoning.effort\"}}"}}
"#;

#[test]
fn failure_message_prefers_the_last_error_event_over_stderr() {
    let message = failure_message(FAILED_EVENTS, "ignored stderr");
    assert!(
        message.contains("Unsupported value: 'minimal'"),
        "{message}"
    );
    assert!(message.contains("reasoning.effort"), "{message}");
}

#[test]
fn failure_message_without_error_events_falls_back_to_stderr() {
    assert_eq!(failure_message("not json\n", "  boom \n"), "boom");
}

#[test]
fn reasoning_effort_rejected_matches_the_param_name_only() {
    assert!(reasoning_effort_rejected(&failure_message(
        FAILED_EVENTS,
        ""
    )));
    assert!(!reasoning_effort_rejected("rate limit exceeded"));
}

#[test]
fn compose_prompt_wraps_system_in_instructions_before_user() {
    let prompt = compose_prompt("be brief", "what is 2+2");
    assert_eq!(
        prompt,
        "<instructions>\nbe brief\n</instructions>\n\nwhat is 2+2"
    );
}

#[test]
fn exec_args_run_read_only_ephemeral_without_user_config() {
    let args = exec_args("gpt-6-sol", "low", &PathBuf::from("/tmp/w"));
    assert_eq!(args[0], "exec");
    for flag in [
        "--ephemeral",
        "--skip-git-repo-check",
        "--ignore-user-config",
        "--ignore-rules",
        "--json",
    ] {
        assert!(args.iter().any(|a| a == flag), "missing {flag}");
    }
    let joined = args.join(" ");
    assert!(joined.contains("-s read-only"));
    assert!(joined.contains("-C /tmp/w"));
    assert!(joined.contains("-m gpt-6-sol"));
    assert!(joined.contains("-c model_reasoning_effort=low"));
    assert!(joined.contains("--output-schema /tmp/w/schema.json"));
    assert!(joined.contains("-o /tmp/w/last-message.txt"));
    assert_eq!(args.last().map(String::as_str), Some("-"));
}

#[test]
fn completion_payload_with_events_carries_usage_thread_id_and_content() {
    let payload = completion_payload("gpt-6-sol", SAMPLE_EVENTS, r#"{"rule":"x"}"#);
    assert_eq!(
        payload.pointer("/id").and_then(Value::as_str),
        Some("01a136aa-4c78-78b3-b5d8-414445754858")
    );
    assert_eq!(
        payload
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str),
        Some(r#"{"rule":"x"}"#)
    );
    assert_eq!(
        payload
            .pointer("/choices/0/finish_reason")
            .and_then(Value::as_str),
        Some("stop")
    );
    assert_eq!(
        payload
            .pointer("/usage/prompt_tokens")
            .and_then(Value::as_i64),
        Some(14890)
    );
    assert_eq!(
        payload
            .pointer("/usage/completion_tokens")
            .and_then(Value::as_i64),
        Some(25)
    );
    assert_eq!(
        payload
            .pointer("/usage/completion_tokens_details/reasoning_tokens")
            .and_then(Value::as_i64),
        Some(3)
    );
    assert_eq!(
        payload
            .pointer("/usage/prompt_tokens_details/cached_tokens")
            .and_then(Value::as_i64),
        Some(12)
    );
    assert!(payload.pointer("/usage/cost").is_some_and(Value::is_null));
}

#[test]
fn completion_payload_without_turn_completed_has_null_usage_and_id() {
    let payload = completion_payload("gpt-6-sol", "not json\n", "{}");
    assert!(payload.pointer("/id").is_some_and(Value::is_null));
    assert!(
        payload
            .pointer("/usage/prompt_tokens")
            .is_some_and(Value::is_null)
    );
}

#[tokio::test]
async fn run_with_missing_binary_is_a_transport_error() {
    let schema = json!({"type": "object"});
    let request = CodexRequest {
        model: "gpt-6-sol",
        system: "s",
        user: "u",
        schema: &schema,
        reasoning_effort: "low",
        timeout: Duration::from_secs(5),
    };
    let outcome = run("/nonexistent/mike-codex-binary", request).await;
    assert!(
        matches!(outcome, Err(LlmError::Transport(ref m)) if m.contains("could not start")),
        "a missing binary must be a transport error, got {outcome:?}"
    );
}
