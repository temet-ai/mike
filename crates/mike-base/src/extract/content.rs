use serde_json::Value;

pub fn extract_text_from_content(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(arr) => arr
            .iter()
            .filter_map(|block| {
                if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                    block.get("text").and_then(|t| t.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

pub fn has_content_block_type(msg: &Value, block_type: &str) -> bool {
    msg.get("content")
        .and_then(|c| c.as_array())
        .map(|blocks| {
            blocks
                .iter()
                .any(|b| b.get("type").and_then(|t| t.as_str()) == Some(block_type))
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn text_from_string_content() -> anyhow::Result<()> {
        let content = json!("Hello world");
        assert_eq!(extract_text_from_content(&content), "Hello world");
        Ok(())
    }

    #[test]
    fn text_from_array_content() -> anyhow::Result<()> {
        let content = json!([
            {"type": "text", "text": "Hello "},
            {"type": "text", "text": "world"}
        ]);
        assert_eq!(extract_text_from_content(&content), "Hello world");
        Ok(())
    }

    #[test]
    fn text_from_empty_array() -> anyhow::Result<()> {
        let content = json!([]);
        assert_eq!(extract_text_from_content(&content), "");
        Ok(())
    }

    #[test]
    fn text_from_non_text_blocks() -> anyhow::Result<()> {
        let content = json!([
            {"type": "image", "source": {"type": "base64"}},
            {"type": "text", "text": "caption"}
        ]);
        assert_eq!(extract_text_from_content(&content), "caption");
        Ok(())
    }

    #[test]
    fn text_from_null_returns_empty() -> anyhow::Result<()> {
        let content = json!(null);
        assert_eq!(extract_text_from_content(&content), "");
        Ok(())
    }

    #[test]
    fn text_from_number_returns_empty() -> anyhow::Result<()> {
        let content = json!(42);
        assert_eq!(extract_text_from_content(&content), "");
        Ok(())
    }

    #[test]
    fn text_from_blocks_missing_text_field() -> anyhow::Result<()> {
        let content = json!([
            {"type": "text"}
        ]);
        assert_eq!(extract_text_from_content(&content), "");
        Ok(())
    }

    #[test]
    fn has_block_type_present() -> anyhow::Result<()> {
        let msg = json!({
            "role": "assistant",
            "content": [
                {"type": "text", "text": "Hello"},
                {"type": "tool_use", "id": "t1", "name": "bash", "input": {}}
            ]
        });
        assert!(has_content_block_type(&msg, "tool_use"));
        assert!(has_content_block_type(&msg, "text"));
        Ok(())
    }

    #[test]
    fn has_block_type_missing() -> anyhow::Result<()> {
        let msg = json!({
            "role": "assistant",
            "content": [{"type": "text", "text": "Hello"}]
        });
        assert!(!has_content_block_type(&msg, "tool_use"));
        Ok(())
    }

    #[test]
    fn has_block_type_no_content() -> anyhow::Result<()> {
        let msg = json!({"role": "user"});
        assert!(!has_content_block_type(&msg, "text"));
        Ok(())
    }

    #[test]
    fn has_block_type_string_content() -> anyhow::Result<()> {
        let msg = json!({"role": "user", "content": "Hello"});
        assert!(!has_content_block_type(&msg, "text"));
        Ok(())
    }

    #[test]
    fn has_block_type_empty_array() -> anyhow::Result<()> {
        let msg = json!({"role": "user", "content": []});
        assert!(!has_content_block_type(&msg, "text"));
        Ok(())
    }
}
