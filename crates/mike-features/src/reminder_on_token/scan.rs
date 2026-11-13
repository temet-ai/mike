use serde_json::Value;

use super::token::PUA_START;

pub fn locate(value: &Value) -> Option<(usize, usize)> {
    let messages = value.get("messages")?.as_array()?;
    let msg_idx = messages
        .iter()
        .rposition(|m| m.get("role").and_then(Value::as_str) == Some("user"))?;
    let content = messages[msg_idx].get("content")?.as_array()?;
    let block_idx = content.iter().position(|b| {
        b.get("type").and_then(Value::as_str) == Some("text")
            && b.get("text")
                .and_then(Value::as_str)
                .is_some_and(|t| t.contains(PUA_START))
    })?;
    Some((msg_idx, block_idx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tok(inner: &str) -> String {
        format!("{}{inner}{}", '\u{E000}', '\u{E001}')
    }

    #[test]
    fn locates_token_in_sibling_text_block_of_last_user_message() {
        let body = json!({
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "hi"}]},
                {"role": "assistant", "content": [{"type": "text", "text": "reply"}]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "id1", "content": "grep output"},
                    {"type": "text", "text": format!("<system-reminder>\nPostToolUse:Bash hook additional context: {}\n</system-reminder>", tok("abc"))},
                ]},
            ],
        });
        assert_eq!(locate(&body), Some((2, 1)));
    }

    #[test]
    fn returns_none_when_no_token_present() {
        let body = json!({
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "no token here"}]},
            ],
        });
        assert!(locate(&body).is_none());
    }

    #[test]
    fn ignores_token_in_tool_result_content_field() {
        let body = json!({
            "messages": [
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "id1", "content": tok("abc")},
                ]},
            ],
        });
        assert!(locate(&body).is_none());
    }

    #[test]
    fn targets_last_user_message_not_earlier_one() {
        let body = json!({
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": tok("old")}]},
                {"role": "assistant", "content": [{"type": "text", "text": "reply"}]},
                {"role": "user", "content": [{"type": "text", "text": tok("new")}]},
            ],
        });
        assert_eq!(locate(&body), Some((2, 0)));
    }

    #[test]
    fn string_shaped_content_returns_none() {
        let body = json!({
            "messages": [{"role": "user", "content": "plain string"}],
        });
        assert!(locate(&body).is_none());
    }
}
