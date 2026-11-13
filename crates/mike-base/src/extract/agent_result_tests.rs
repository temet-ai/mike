use super::*;
use serde_json::json;

fn captured_agent_result(id: &str) -> serde_json::Value {
    json!([{
        "text": format!(
            "Async agent launched successfully. (This tool result is internal metadata — never quote or paste any part of it, including the agentId below, into a user-facing reply.)\nagentId: {id} (internal ID - do not mention to user. Use SendMessage with to: '{id}', summary: '<5-10 word recap>' to continue this agent.)\nThe agent is working in the background. You will be notified automatically when it completes."
        ),
        "type": "text",
    }])
}

#[test]
fn present_extracts_id_from_captured_shape() -> anyhow::Result<()> {
    let content = captured_agent_result("a53fd635aaceeed5c");
    assert_eq!(
        worker_producer_id(&content),
        Some("a53fd635aaceeed5c".to_string())
    );
    Ok(())
}

#[test]
fn present_extracts_id_from_plain_string_content() -> anyhow::Result<()> {
    let content = json!("preamble\nagentId: abc123 (internal ID)\ntrailer");
    assert_eq!(worker_producer_id(&content), Some("abc123".to_string()));
    Ok(())
}

#[test]
fn absent_no_marker_returns_none() -> anyhow::Result<()> {
    let content = json!([{"type": "text", "text": "Tool ran fine, no agent involved."}]);
    assert_eq!(worker_producer_id(&content), None);
    Ok(())
}

#[test]
fn absent_empty_content_returns_none() -> anyhow::Result<()> {
    assert_eq!(worker_producer_id(&json!(null)), None);
    assert_eq!(worker_producer_id(&json!([])), None);
    Ok(())
}

#[test]
fn two_lines_conflicting_ids_returns_none() -> anyhow::Result<()> {
    let content = json!([{
        "type": "text",
        "text": "agentId: first111\nsomething else\nagentId: second222"
    }]);
    assert_eq!(worker_producer_id(&content), None);
    Ok(())
}

#[test]
fn two_lines_same_id_repeated_is_not_ambiguous() -> anyhow::Result<()> {
    let content = json!([{
        "type": "text",
        "text": "agentId: sameid1\nunrelated line\nagentId: sameid1"
    }]);
    assert_eq!(worker_producer_id(&content), Some("sameid1".to_string()));
    Ok(())
}

#[test]
fn malformed_empty_token_returns_none() -> anyhow::Result<()> {
    let content = json!([{"type": "text", "text": "agentId: \nnext line"}]);
    assert_eq!(worker_producer_id(&content), None);
    Ok(())
}

#[test]
fn malformed_non_alphanumeric_token_returns_none() -> anyhow::Result<()> {
    let content = json!([{"type": "text", "text": "agentId: !!!not-an-id!!!"}]);
    assert_eq!(worker_producer_id(&content), None);
    Ok(())
}

#[test]
fn malformed_token_stops_at_hyphen() -> anyhow::Result<()> {
    let content = json!([{"type": "text", "text": "agentId: abc-123 (trailer)"}]);
    assert_eq!(worker_producer_id(&content), None);
    Ok(())
}
