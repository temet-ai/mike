use std::collections::HashMap;

use serde_json::{Value, json};

use crate::wire::manifest::sha256_hex;

pub struct CapturedResults {
    entries: HashMap<String, Vec<(String, String)>>,
}

fn content(block: &Value) -> Option<(&str, &'static str)> {
    match block.get("content")? {
        Value::String(text) => Some((text, "content")),
        Value::Array(parts) if parts.len() == 1 && parts[0].get("type")?.as_str()? == "text" => {
            Some((parts[0].get("text")?.as_str()?, "content/0/text"))
        }
        _ => None,
    }
}

impl CapturedResults {
    pub fn new(raw: &[u8]) -> Self {
        let mut entries: HashMap<String, Vec<(String, String)>> = HashMap::new();
        if let Ok(body) = serde_json::from_slice::<Value>(raw) {
            for (mi, message) in body["messages"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
            {
                for (bi, block) in message["content"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .enumerate()
                {
                    if block["type"] != "tool_result" {
                        continue;
                    }
                    if let (Some(id), Some((text, suffix))) =
                        (block["tool_use_id"].as_str(), content(block))
                    {
                        entries.entry(id.into()).or_default().push((
                            sha256_hex(text.as_bytes()),
                            format!("/messages/{mi}/content/{bi}/{suffix}"),
                        ));
                    }
                }
            }
        }
        Self { entries }
    }

    pub fn pointer(&self, tool_use_id: &str, sha256: &str) -> Option<&str> {
        let entries = self.entries.get(tool_use_id)?;
        let mut matching = entries.iter().filter(|(hash, _)| hash == sha256);
        let (_, pointer) = matching.next()?;
        matching.next().is_none().then_some(pointer)
    }

    pub fn reference(&self, tool_use_id: &str, text: &str) -> Option<String> {
        let sha256 = sha256_hex(text.as_bytes());
        self.pointer(tool_use_id, &sha256)?;
        Some(json!({"tool_use_id": tool_use_id, "sha256": sha256}).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captured_recovery_preserves_unicode_whitespace_and_rejects_other_versions()
    -> anyhow::Result<()> {
        let text = "original\r\n  α\t\"quoted\"\n";
        let body = json!({"messages":[{"content":[{"type":"tool_result","tool_use_id":"one","content":text}]}]});
        let raw = serde_json::to_vec(&body)?;
        let capture = CapturedResults::new(&raw);
        let args: Value = serde_json::from_str(
            &capture
                .reference("one", text)
                .ok_or_else(|| anyhow::anyhow!("missing reference"))?,
        )?;
        assert!(args.get("id").is_none());
        assert_eq!(args["tool_use_id"], "one");
        let pointer = capture
            .pointer("one", &sha256_hex(text.as_bytes()))
            .ok_or_else(|| anyhow::anyhow!("missing original"))?;
        assert_eq!(body.pointer(pointer).and_then(Value::as_str), Some(text));
        assert!(capture.reference("one", "changed file").is_none());
        assert!(capture.reference("other", text).is_none());
        Ok(())
    }

    #[test]
    fn captured_recovery_refuses_ambiguity_and_accepts_single_text_blocks() {
        let block = json!({"type":"tool_result","tool_use_id":"one","content":[{"type":"text","text":"old"}]});
        let raw = json!({"messages":[{"content":[block.clone()]}]}).to_string();
        assert!(
            CapturedResults::new(raw.as_bytes())
                .reference("one", "old")
                .is_some()
        );
        let duplicate = json!({"messages":[{"content":[block.clone(),block]}]}).to_string();
        assert!(
            CapturedResults::new(duplicate.as_bytes())
                .reference("one", "old")
                .is_none()
        );
        assert!(
            CapturedResults::new(b"bad")
                .reference("one", "old")
                .is_none()
        );
    }
}
