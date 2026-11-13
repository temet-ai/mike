use serde_json::Value;

const AWAY_RECAP_PREFIX: &str = "The user stepped away and is coming back. Recap in under 40 words";

pub fn is_away_recap_call(body_bytes: &[u8]) -> bool {
    let Ok(value) = serde_json::from_slice::<Value>(body_bytes) else {
        return false;
    };
    let Some(last_user) = value
        .get("messages")
        .and_then(Value::as_array)
        .and_then(|messages| {
            messages
                .iter()
                .rev()
                .find(|m| m.get("role").and_then(Value::as_str) == Some("user"))
        })
    else {
        return false;
    };
    match last_user.get("content") {
        Some(Value::String(text)) => text.starts_with(AWAY_RECAP_PREFIX),
        Some(Value::Array(blocks)) => blocks.iter().any(|block| {
            block
                .get("text")
                .and_then(Value::as_str)
                .is_some_and(|text| text.starts_with(AWAY_RECAP_PREFIX))
        }),
        _ => false,
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use serde_json::json;

    fn body(content: Value) -> Vec<u8> {
        serde_json::to_vec(&json!({"messages": [{"role": "user", "content": content}]}))
            .unwrap_or_default()
    }

    #[test]
    fn recap_string_content_is_recognised() {
        let text = format!("{AWAY_RECAP_PREFIX}: what happened?");
        assert!(is_away_recap_call(&body(json!(text))));
    }

    #[test]
    fn recap_block_content_is_recognised() {
        let text = format!("{AWAY_RECAP_PREFIX}: what happened?");
        assert!(is_away_recap_call(&body(
            json!([{"type": "text", "text": text}])
        )));
    }

    #[test]
    fn ordinary_prompt_is_not_a_recap() {
        assert!(!is_away_recap_call(&body(json!("fix the bug"))));
        assert!(!is_away_recap_call(b"not json"));
    }
}
