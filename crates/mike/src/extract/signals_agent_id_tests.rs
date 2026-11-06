use super::*;

fn parts_with_agent_id(value: &str) -> anyhow::Result<Parts> {
    let (parts, ()) = axum::http::Request::builder()
        .method("POST")
        .uri("https://api.anthropic.com/v1/messages")
        .header("x-claude-code-agent-id", value)
        .body(())?
        .into_parts();
    Ok(parts)
}

#[test]
fn blank_agent_id_header_is_none() -> anyhow::Result<()> {
    for value in ["", "   ", "\t"] {
        assert_eq!(
            extract_claude_code_agent_id(&parts_with_agent_id(value)?),
            None,
            "value {value:?}"
        );
    }
    Ok(())
}

#[test]
fn agent_id_header_is_trimmed() -> anyhow::Result<()> {
    assert_eq!(
        extract_claude_code_agent_id(&parts_with_agent_id(" agent-7 ")?).as_deref(),
        Some("agent-7")
    );
    Ok(())
}
