use serde_json::{Value, json};

pub fn inject_after_trigger(
    messages: &mut [Value],
    token: &str,
    reminder_text: &str,
) -> bool {
    let Some(trigger_idx) = latest_trigger_index(messages, token) else {
        return false;
    };
    for msg in messages.iter_mut().skip(trigger_idx + 1) {
        let Some(obj) = msg.as_object_mut() else {
            continue;
        };
        if obj.get("role").and_then(Value::as_str) != Some("user") {
            continue;
        }
        let Some(content) = obj.get_mut("content").and_then(Value::as_array_mut) else {
            return false;
        };
        content.push(json!({
            "type": "text",
            "text": reminder_text,
        }));
        return true;
    }
    false
}

fn latest_trigger_index(messages: &[Value], token: &str) -> Option<usize> {
    if token.is_empty() {
        return None;
    }
    let (idx, msg) = messages.iter().enumerate().rev().find(|(_, m)| {
        m.as_object()
            .and_then(|o| o.get("role"))
            .and_then(Value::as_str)
            == Some("assistant")
    })?;
    assistant_text_contains(msg, token).then_some(idx)
}

fn assistant_text_contains(msg: &Value, token: &str) -> bool {
    let Some(content) = msg.get("content").and_then(Value::as_array) else {
        return false;
    };
    content.iter().any(|block| {
        block.get("type").and_then(Value::as_str) == Some("text")
            && block
                .get("text")
                .and_then(Value::as_str)
                .is_some_and(|t| t.contains(token))
    })
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use serde_json::json;

    const TOKEN: &str = "!!CHROME!!";
    const REMINDER: &str = "<system-reminder>\nsection text\n</system-reminder>";

    fn assistant_text(text: &str) -> Value {
        json!({"role": "assistant", "content": [{"type": "text", "text": text}]})
    }

    fn user_blocks(blocks: Vec<Value>) -> Value {
        json!({"role": "user", "content": blocks})
    }

    fn take_messages(v: Value) -> anyhow::Result<Vec<Value>> {
        v.as_array()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("fixture must be an array"))
    }

    #[test]
    fn trigger_in_last_assistant_turn_appends_to_last_user_message() -> anyhow::Result<()> {
        let mut messages = take_messages(json!([
            {"role": "user", "content": [{"type": "text", "text": "open the page"}]},
            assistant_text(&format!("On it. {TOKEN}")),
            user_blocks(vec![json!({"type": "tool_result", "tool_use_id": "t1", "content": "tabs"})]),
        ]))?;
        assert!(inject_after_trigger(&mut messages, TOKEN, REMINDER));
        let content = messages[2]["content"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("content must be array"))?;
        assert_eq!(content.len(), 2, "tool_result + appended reminder");
        assert_eq!(content[1]["type"], "text");
        assert_eq!(content[1]["text"], REMINDER);
        Ok(())
    }

    #[test]
    fn trigger_only_in_older_assistant_turn_is_noop() -> anyhow::Result<()> {
        let mut messages = take_messages(json!([
            {"role": "user", "content": [{"type": "text", "text": "q1"}]},
            assistant_text(&format!("triggering {TOKEN} now")),
            user_blocks(vec![json!({"type": "text", "text": "tool turn"})]),
            assistant_text("later reply, no token"),
            {"role": "user", "content": [{"type": "text", "text": "latest"}]},
        ]))?;
        let before = messages.clone();
        assert!(!inject_after_trigger(&mut messages, TOKEN, REMINDER));
        assert_eq!(
            messages, before,
            "an older trigger must not re-inject once a newer assistant turn exists"
        );
        Ok(())
    }

    #[test]
    fn token_in_latest_turn_injects_after_it_even_if_present_earlier() -> anyhow::Result<()> {
        let mut messages = take_messages(json!([
            assistant_text(&format!("first {TOKEN}")),
            user_blocks(vec![json!({"type": "text", "text": "turn A"})]),
            assistant_text(&format!("again {TOKEN}")),
            user_blocks(vec![json!({"type": "text", "text": "turn B"})]),
        ]))?;
        assert!(inject_after_trigger(&mut messages, TOKEN, REMINDER));
        let a = messages[1]["content"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("content must be array"))?;
        let b = messages[3]["content"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("content must be array"))?;
        assert_eq!(a.len(), 1, "earlier user turn untouched");
        assert_eq!(b.len(), 2, "injected after the LATEST occurrence");
        assert_eq!(b[1]["text"], REMINDER);
        Ok(())
    }

    #[test]
    fn no_user_message_after_trigger_is_noop() -> anyhow::Result<()> {
        let mut messages = take_messages(json!([
            {"role": "user", "content": [{"type": "text", "text": "q"}]},
            assistant_text(&format!("plain text turn end {TOKEN}")),
        ]))?;
        let before = messages.clone();
        assert!(!inject_after_trigger(&mut messages, TOKEN, REMINDER));
        assert_eq!(messages, before, "messages must be untouched");
        Ok(())
    }

    #[test]
    fn string_shaped_user_content_after_trigger_is_noop() -> anyhow::Result<()> {
        let mut messages = take_messages(json!([
            assistant_text(&format!("x {TOKEN}")),
            {"role": "user", "content": "legacy string-shaped turn"},
        ]))?;
        let before = messages.clone();
        assert!(!inject_after_trigger(&mut messages, TOKEN, REMINDER));
        assert_eq!(messages, before);
        Ok(())
    }

    #[test]
    fn token_in_user_message_does_not_fire() -> anyhow::Result<()> {
        let mut messages = take_messages(json!([
            {"role": "user", "content": [{"type": "text", "text": format!("the doc says {TOKEN}")}]},
            assistant_text("no token here"),
            {"role": "user", "content": [{"type": "text", "text": "follow-up"}]},
        ]))?;
        let before = messages.clone();
        assert!(!inject_after_trigger(&mut messages, TOKEN, REMINDER));
        assert_eq!(messages, before);
        Ok(())
    }

    #[test]
    fn string_shaped_assistant_content_is_not_scanned() -> anyhow::Result<()> {
        let mut messages = take_messages(json!([
            {"role": "assistant", "content": format!("string turn {TOKEN}")},
            {"role": "user", "content": [{"type": "text", "text": "next"}]},
        ]))?;
        let before = messages.clone();
        assert!(!inject_after_trigger(&mut messages, TOKEN, REMINDER));
        assert_eq!(messages, before);
        Ok(())
    }

    #[test]
    fn token_in_non_text_assistant_block_does_not_fire() -> anyhow::Result<()> {
        let mut messages = take_messages(json!([
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "x", "input": {"arg": TOKEN}}
            ]},
            {"role": "user", "content": [{"type": "text", "text": "next"}]},
        ]))?;
        let before = messages.clone();
        assert!(!inject_after_trigger(&mut messages, TOKEN, REMINDER));
        assert_eq!(messages, before);
        Ok(())
    }

    #[test]
    fn empty_token_never_fires() -> anyhow::Result<()> {
        let mut messages = take_messages(json!([
            assistant_text("anything"),
            {"role": "user", "content": [{"type": "text", "text": "next"}]},
        ]))?;
        assert!(!inject_after_trigger(&mut messages, "", REMINDER));
        Ok(())
    }
}
