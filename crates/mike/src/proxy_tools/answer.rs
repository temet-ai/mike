use anyhow::Context;
use bytes::Bytes;
use serde_json::Value;

use crate::config::AppState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProxyToolCall {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) input_json: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct AssistantPrelude {
    pub(crate) text: String,
}

pub(crate) async fn answer_tool_call(
    state: &AppState,
    call: &ProxyToolCall,
    project_path: &str,
) -> String {
    match call.name.as_str() {
        crate::proxy_tools::roster::TOOL_NAME => {
            crate::proxy_tools::roster::answer(state, project_path).await
        }
        other => format!(
            "The mike proxy declared no tool named {other}, so it has nothing to answer with."
        ),
    }
}

pub(crate) fn build_followup_body(
    forwarded_body: &[u8],
    prelude: &AssistantPrelude,
    call: &ProxyToolCall,
    tool_result: &str,
) -> anyhow::Result<Bytes> {
    let mut json: Value = serde_json::from_slice(forwarded_body)
        .context("proxy tool loop: forwarded request body is not JSON")?;
    let obj = json
        .as_object_mut()
        .context("proxy tool loop: forwarded request body is not a JSON object")?;
    let messages = obj
        .entry("messages")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .context("proxy tool loop: 'messages' is not an array")?;

    let mut assistant_content: Vec<Value> = Vec::new();
    if !prelude.text.trim().is_empty() {
        assistant_content.push(serde_json::json!({"type": "text", "text": prelude.text}));
    }
    assistant_content.push(serde_json::json!({
        "type": "tool_use",
        "id": call.id,
        "name": call.name,
        "input": parse_input(&call.input_json),
    }));
    messages.push(serde_json::json!({
        "role": "assistant",
        "content": assistant_content,
    }));

    messages.push(serde_json::json!({
        "role": "user",
        "content": [{
            "type": "tool_result",
            "tool_use_id": call.id,
            "content": tool_result,
        }],
    }));

    Ok(Bytes::from(serde_json::to_vec(&json).context(
        "proxy tool loop: failed to serialize follow-up body",
    )?))
}

fn parse_input(input_json: &str) -> Value {
    let trimmed = input_json.trim();
    if trimmed.is_empty() {
        return serde_json::json!({});
    }
    serde_json::from_str(trimmed).unwrap_or_else(|_| serde_json::json!({}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call() -> ProxyToolCall {
        ProxyToolCall {
            id: "toolu_01ABC".to_string(),
            name: crate::proxy_tools::roster::TOOL_NAME.to_string(),
            input_json: String::new(),
        }
    }

    fn body() -> &'static [u8] {
        br#"{"model":"m","messages":[{"role":"user","content":"who else is here?"}],"system":"S","tools":[{"name":"Read"}]}"#
    }

    #[test]
    fn appends_an_assistant_tool_use_turn_and_a_user_tool_result_turn() -> anyhow::Result<()> {
        let out = build_followup_body(body(), &AssistantPrelude::default(), &call(), "nobody")?;
        let v: Value = serde_json::from_slice(&out)?;
        let messages = v["messages"].as_array().context("messages")?;
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(messages[1]["content"][0]["type"], "tool_use");
        assert_eq!(messages[1]["content"][0]["id"], "toolu_01ABC");
        assert_eq!(messages[2]["role"], "user");
        assert_eq!(messages[2]["content"][0]["type"], "tool_result");
        assert_eq!(messages[2]["content"][0]["content"], "nobody");
        Ok(())
    }

    #[test]
    fn the_tool_result_echoes_the_tool_use_id() -> anyhow::Result<()> {
        let out = build_followup_body(body(), &AssistantPrelude::default(), &call(), "x")?;
        let v: Value = serde_json::from_slice(&out)?;
        assert_eq!(
            v["messages"][1]["content"][0]["id"],
            v["messages"][2]["content"][0]["tool_use_id"]
        );
        Ok(())
    }

    #[test]
    fn assistant_text_before_the_call_travels_into_the_followup() -> anyhow::Result<()> {
        let prelude = AssistantPrelude {
            text: "Let me check who else is working here.".to_string(),
        };
        let out = build_followup_body(body(), &prelude, &call(), "x")?;
        let v: Value = serde_json::from_slice(&out)?;
        let content = v["messages"][1]["content"].as_array().context("content")?;
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[0]["text"], "Let me check who else is working here.");
        assert_eq!(content[1]["type"], "tool_use");
        Ok(())
    }

    #[test]
    fn empty_assistant_text_produces_a_tool_use_only_turn() -> anyhow::Result<()> {
        let prelude = AssistantPrelude {
            text: "   \n ".to_string(),
        };
        let out = build_followup_body(body(), &prelude, &call(), "x")?;
        let v: Value = serde_json::from_slice(&out)?;
        assert_eq!(
            v["messages"][1]["content"].as_array().context("c")?.len(),
            1
        );
        Ok(())
    }

    #[test]
    fn the_system_prompt_and_declared_tools_survive() -> anyhow::Result<()> {
        let out = build_followup_body(body(), &AssistantPrelude::default(), &call(), "x")?;
        let v: Value = serde_json::from_slice(&out)?;
        assert_eq!(v["system"], "S");
        assert_eq!(v["model"], "m");
        assert_eq!(v["tools"][0]["name"], "Read");
        Ok(())
    }

    #[test]
    fn a_no_argument_call_reproduces_an_empty_input_object() {
        assert_eq!(parse_input(""), serde_json::json!({}));
        assert_eq!(parse_input("  "), serde_json::json!({}));
    }

    #[test]
    fn a_well_formed_input_is_reproduced_verbatim() {
        assert_eq!(
            parse_input(r#"{"scope":"live"}"#),
            serde_json::json!({"scope": "live"})
        );
    }

    #[test]
    fn a_truncated_input_degrades_to_an_empty_object() {
        assert_eq!(parse_input(r#"{"scope":"#), serde_json::json!({}));
    }

    #[test]
    fn a_body_that_is_not_json_is_an_error_rather_than_a_silent_empty_request() {
        assert!(build_followup_body(b"nope", &AssistantPrelude::default(), &call(), "x").is_err());
    }
}
