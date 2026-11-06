use super::*;
use serde_json::json;

#[test]
fn message_count_normal() -> anyhow::Result<()> {
    let body = json!({
        "messages": [
            {"role": "user", "content": "Hi"},
            {"role": "assistant", "content": "Hey"},
            {"role": "user", "content": "Bye"}
        ]
    });
    assert_eq!(extract_message_count(&body), 3);
    Ok(())
}

#[test]
fn message_count_empty() -> anyhow::Result<()> {
    let body = json!({"messages": []});
    assert_eq!(extract_message_count(&body), 0);
    Ok(())
}

#[test]
fn message_count_missing() -> anyhow::Result<()> {
    let body = json!({});
    assert_eq!(extract_message_count(&body), 0);
    Ok(())
}

#[test]
fn message_count_wrong_type() -> anyhow::Result<()> {
    let body = json!({"messages": "not an array"});
    assert_eq!(extract_message_count(&body), 0);
    Ok(())
}

#[test]
fn tool_names_present() -> anyhow::Result<()> {
    let body = json!({
        "tools": [
            {"name": "bash", "description": "Run bash"},
            {"name": "read_file", "description": "Read a file"}
        ]
    });
    assert_eq!(extract_tool_names(&body), vec!["bash", "read_file"]);
    Ok(())
}

#[test]
fn tool_names_empty() -> anyhow::Result<()> {
    let body = json!({"tools": []});
    assert!(extract_tool_names(&body).is_empty());
    Ok(())
}

#[test]
fn tool_names_missing() -> anyhow::Result<()> {
    let body = json!({});
    assert!(extract_tool_names(&body).is_empty());
    Ok(())
}

#[test]
fn tool_names_some_missing_name_field() -> anyhow::Result<()> {
    let body = json!({
        "tools": [
            {"name": "bash"},
            {"description": "no name here"},
            {"name": "edit"}
        ]
    });
    assert_eq!(extract_tool_names(&body), vec!["bash", "edit"]);
    Ok(())
}

#[test]
fn tool_names_wrong_type() -> anyhow::Result<()> {
    let body = json!({"tools": "not an array"});
    assert!(extract_tool_names(&body).is_empty());
    Ok(())
}

#[test]
fn params_all_present() -> anyhow::Result<()> {
    let body = json!({
        "max_tokens": 4096,
        "temperature": 0.7,
        "thinking": {
            "type": "enabled",
            "budget_tokens": 10000
        }
    });
    let params = extract_params(&body);
    assert_eq!(params.max_tokens, Some(4096));
    assert!((params.temperature.unwrap_or(0.0) - 0.7).abs() < f32::EPSILON);
    assert!(params.thinking_enabled);
    assert_eq!(params.thinking_budget, Some(10000));
    assert_eq!(params.thinking_mode.as_deref(), Some("enabled"));
    Ok(())
}

#[test]
fn params_all_missing() -> anyhow::Result<()> {
    let body = json!({});
    let params = extract_params(&body);
    assert_eq!(params.max_tokens, None);
    assert_eq!(params.temperature, None);
    assert!(!params.thinking_enabled);
    assert_eq!(params.thinking_budget, None);
    assert_eq!(params.thinking_mode, None);
    Ok(())
}

#[test]
fn params_thinking_disabled() -> anyhow::Result<()> {
    let body = json!({
        "thinking": {"type": "disabled"}
    });
    let params = extract_params(&body);
    assert!(!params.thinking_enabled);
    assert_eq!(params.thinking_budget, None);
    assert_eq!(params.thinking_mode.as_deref(), Some("disabled"));
    Ok(())
}

#[test]
fn params_thinking_enabled_no_budget() -> anyhow::Result<()> {
    let body = json!({
        "thinking": {"type": "enabled"}
    });
    let params = extract_params(&body);
    assert!(params.thinking_enabled);
    assert_eq!(params.thinking_budget, None);
    assert_eq!(params.thinking_mode.as_deref(), Some("enabled"));
    Ok(())
}

#[test]
fn params_thinking_adaptive() -> anyhow::Result<()> {
    let body = json!({
        "thinking": {"type": "adaptive"}
    });
    let params = extract_params(&body);
    assert!(params.thinking_enabled);
    assert_eq!(params.thinking_budget, None);
    assert_eq!(params.thinking_mode.as_deref(), Some("adaptive"));
    Ok(())
}

#[test]
fn params_effort_read_from_output_config() -> anyhow::Result<()> {
    let body = json!({
        "output_config": {"effort": "high"}
    });
    let params = extract_params(&body);
    assert_eq!(params.effort.as_deref(), Some("high"));
    Ok(())
}

#[test]
fn params_effort_read_from_interactive_claude_code_body() -> anyhow::Result<()> {
    let body = json!({
        "model": "claude-fable-5-1",
        "max_tokens": 64000,
        "stream": true,
        "thinking": {"type": "adaptive"},
        "output_config": {"effort": "xhigh"},
        "system": [{"type": "text", "text": "You are Claude Code"}],
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "hello"}]}
        ]
    });
    let params = extract_params(&body);
    assert_eq!(params.effort.as_deref(), Some("xhigh"));
    assert_eq!(params.max_tokens, Some(64000));
    assert_eq!(params.thinking_mode.as_deref(), Some("adaptive"));
    Ok(())
}

#[test]
fn params_effort_mid_conversation_change_before_newest_user_turn_wins() -> anyhow::Result<()> {
    let body = json!({
        "output_config": {"effort": "xhigh"},
        "messages": [
            {"role": "user", "content": "a"},
            {"role": "system", "content": [], "output_config": {"effort": "low"}},
            {"role": "assistant", "content": "b"},
            {"role": "system", "content": [], "output_config": {"effort": "medium"}},
            {"role": "user", "content": "c"}
        ]
    });
    assert_eq!(extract_params(&body).effort.as_deref(), Some("medium"));
    Ok(())
}

#[test]
fn params_effort_change_after_newest_user_turn_is_not_yet_in_force() -> anyhow::Result<()> {
    let body = json!({
        "output_config": {"effort": "high"},
        "messages": [
            {"role": "user", "content": "a"},
            {"role": "system", "content": [], "output_config": {"effort": "low"}}
        ]
    });
    assert_eq!(extract_params(&body).effort.as_deref(), Some("high"));
    Ok(())
}

#[test]
fn params_effort_absent_without_output_config() -> anyhow::Result<()> {
    let params = extract_params(&json!({"max_tokens": 10}));
    assert_eq!(params.effort, None);
    Ok(())
}

#[test]
fn params_thinking_unknown_mode() -> anyhow::Result<()> {
    let body = json!({
        "thinking": {"type": "extended-thinking-2"}
    });
    let params = extract_params(&body);
    assert!(params.thinking_enabled);
    assert_eq!(params.thinking_mode.as_deref(), Some("extended-thinking-2"));
    Ok(())
}

#[test]
fn params_max_tokens_wrong_type() -> anyhow::Result<()> {
    let body = json!({"max_tokens": "not a number"});
    let params = extract_params(&body);
    assert_eq!(params.max_tokens, None);
    Ok(())
}

#[test]
fn params_temperature_wrong_type() -> anyhow::Result<()> {
    let body = json!({"temperature": "warm"});
    let params = extract_params(&body);
    assert_eq!(params.temperature, None);
    Ok(())
}
