use serde::Deserialize;
use serde_json::value::RawValue;

#[derive(Deserialize)]
struct Conversation<'a> {
    #[serde(borrow)]
    messages: Vec<Message<'a>>,
}

#[derive(Deserialize)]
struct Message<'a> {
    role: &'a str,
    #[serde(borrow)]
    content: &'a RawValue,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Content {
    Text(String),
    Blocks(Vec<Block>),
}

#[derive(Deserialize)]
struct Block {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

pub(super) fn conversation_query(body: &[u8]) -> (String, usize) {
    let (mut prompts, count) = recent_user_prompts(body, 3);
    prompts.reverse();
    (prompts.join("\n"), count)
}

pub(crate) fn recent_user_prompts(body: &[u8], limit: usize) -> (Vec<String>, usize) {
    let Ok(conversation) = serde_json::from_slice::<Conversation<'_>>(body) else {
        return (Vec::new(), 0);
    };
    let prompts = conversation
        .messages
        .iter()
        .rev()
        .filter(|message| message.role == "user")
        .filter_map(|message| user_text(message.content))
        .take(limit)
        .collect();
    (prompts, conversation.messages.len())
}

fn user_text(content: &RawValue) -> Option<String> {
    let content = serde_json::from_str::<Content>(content.get()).ok()?;
    let text = match content {
        Content::Text(text) => text,
        Content::Blocks(blocks) => blocks
            .into_iter()
            .filter(|block| block.kind == "text")
            .filter_map(|block| block.text)
            .collect::<Vec<_>>()
            .join("\n"),
    };
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_results_do_not_change_conversation_query() {
        let body = br#"{"messages":[{"role":"user","content":"cargo checks"},{"role":"assistant","content":[{"type":"tool_use","input":{"large":"irrelevant"}}]},{"role":"user","content":[{"type":"tool_result","content":"not a human prompt"}]}]}"#;
        assert_eq!(conversation_query(body), ("cargo checks".into(), 3));
    }

    #[test]
    fn malformed_body_has_no_query() {
        assert_eq!(conversation_query(b"invalid"), (String::new(), 0));
    }
}
