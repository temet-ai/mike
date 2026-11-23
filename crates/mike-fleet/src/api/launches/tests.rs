use serde_json::json;

use super::{CreateLaunchRequest, ServerOpenRequest};
use crate::runtime::RuntimeKind;

#[test]
fn codex_request_preserves_runtime_arguments_and_identity() -> anyhow::Result<()> {
    let request: CreateLaunchRequest = serde_json::from_value(json!({
        "folder": "/repo", "runtime": "codex", "args": ["--model", "chosen-model"],
        "name": "codex-worker", "goal": "finish the change"
    }))?;
    let request = ServerOpenRequest::from(request);
    assert_eq!(request.runtime, RuntimeKind::Codex);
    assert_eq!(request.claude_args, ["--model", "chosen-model"]);
    assert_eq!(request.session_name.as_deref(), Some("codex-worker"));
    assert_eq!(request.goal.as_deref(), Some("finish the change"));
    Ok(())
}

#[test]
fn legacy_request_keeps_claude_as_default() -> anyhow::Result<()> {
    let request: CreateLaunchRequest = serde_json::from_value(json!({
        "folder": "/repo", "claude_args": ["--model", "sonnet"]
    }))?;
    assert_eq!(request.runtime, RuntimeKind::Claude);
    assert_eq!(request.claude_args, ["--model", "sonnet"]);
    Ok(())
}

#[test]
fn conflicting_argument_aliases_and_unknown_runtime_are_rejected() {
    for value in [
        json!({"folder": "/repo", "args": [], "claude_args": []}),
        json!({"folder": "/repo", "runtime": "unknown"}),
    ] {
        assert!(serde_json::from_value::<CreateLaunchRequest>(value).is_err());
    }
}
