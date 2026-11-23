use super::*;

#[test]
fn caller_env_keeps_only_present_terminal_keys() {
    let env = caller_env(|key| match key {
        "HC_TERMINAL_ID" => Some("t13".to_string()),
        "TMUX" => Some(String::new()),
        _ => None,
    });
    assert_eq!(Value::Object(env), json!({"HC_TERMINAL_ID": "t13"}));
}

#[test]
fn caller_env_outside_any_terminal_is_empty() {
    assert!(caller_env(|_| None).is_empty());
}

#[test]
fn codex_spawn_body_preserves_runtime_args_and_identity() -> anyhow::Result<()> {
    let args = serde_json::from_value::<SpawnSessionArgs>(json!({
        "folder": "/repo", "runtime": "codex", "args": ["--model", "chosen-model"],
        "name": "codex-worker", "goal": "finish the change"
    }))?;
    let body = spawn_session_body(args);
    assert_eq!(body["runtime"], "codex");
    assert_eq!(body["args"], json!(["--model", "chosen-model"]));
    assert_eq!(body["name"], "codex-worker");
    assert_eq!(body["goal"], "finish the change");
    assert!(body.get("claude_args").is_none());
    Ok(())
}

#[test]
fn legacy_args_still_default_to_claude() -> anyhow::Result<()> {
    let args = serde_json::from_value::<SpawnSessionArgs>(json!({
        "folder": "/repo", "claude_args": ["--model", "sonnet"]
    }))?;
    assert_eq!(args.runtime, SpawnRuntime::Claude);
    assert_eq!(
        spawn_session_body(args)["args"],
        json!(["--model", "sonnet"])
    );
    Ok(())
}

#[test]
fn conflicting_aliases_and_unknown_runtime_fail_before_launch() {
    for value in [
        json!({"folder": "/repo", "args": [], "claude_args": []}),
        json!({"folder": "/repo", "runtime": "unknown"}),
    ] {
        assert!(serde_json::from_value::<SpawnSessionArgs>(value).is_err());
    }
}

#[test]
fn spawn_schema_exposes_both_runtimes_and_legacy_argument_alias() {
    let schema = spawn_session_tool().input_schema;
    assert_eq!(
        schema["properties"]["runtime"]["enum"],
        json!(["claude", "codex"])
    );
    assert_eq!(schema["properties"]["runtime"]["default"], "claude");
    assert_eq!(schema["properties"]["claude_args"]["deprecated"], true);
    assert_eq!(schema["properties"]["args"]["type"], "array");
}
