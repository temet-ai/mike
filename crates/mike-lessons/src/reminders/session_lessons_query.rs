use crate::conversation::EarlierTurns;
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

pub fn conversation_query(body: &[u8], earlier: &EarlierTurns) -> (String, usize) {
    let (mut prompts, count) = recent_user_prompts(body, earlier, 3);
    prompts.reverse();
    (prompts.join("\n"), count)
}

pub fn recent_user_prompts(
    body: &[u8],
    earlier: &EarlierTurns,
    limit: usize,
) -> (Vec<String>, usize) {
    let Ok(conversation) = serde_json::from_slice::<Conversation<'_>>(body) else {
        return (Vec::new(), 0);
    };
    let from_body = conversation
        .messages
        .iter()
        .rev()
        .filter(|message| message.role == "user")
        .filter_map(|message| user_text(message.content));
    let from_earlier = earlier
        .raw()
        .rev()
        .filter_map(|raw| serde_json::from_str::<Message<'_>>(raw.get()).ok())
        .filter(|message| message.role == "user")
        .filter_map(|message| user_text(message.content));
    let prompts = from_body.chain(from_earlier).take(limit).collect();
    (prompts, earlier.len() + conversation.messages.len())
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
        assert_eq!(
            conversation_query(body, &crate::conversation::WHOLE_BODY),
            ("cargo checks".into(), 3)
        );
    }

    #[test]
    fn conversation_query_continue_body_with_earlier_turns_matches_full_body() -> anyhow::Result<()>
    {
        let full = br#"{"messages":[{"role":"user","content":"cargo checks"},{"role":"assistant","content":[{"type":"tool_use","input":{"large":"irrelevant"}}]},{"role":"user","content":[{"type":"tool_result","content":"not a human prompt"}]}]}"#;
        let continue_body = br#"{"thread":{"type":"continue","previous_message_id":"msg_1"},"messages":[{"role":"user","content":[{"type":"tool_result","content":"not a human prompt"}]}]}"#;
        let earlier = EarlierTurns::from_chain(std::sync::Arc::from(
            crate::conversation::body_messages(
                br#"{"messages":[{"role":"user","content":"cargo checks"},{"role":"assistant","content":[{"type":"tool_use","input":{"large":"irrelevant"}}]}]}"#,
            )
            .unwrap_or_default(),
        ));
        assert_eq!(
            conversation_query(continue_body, &earlier),
            conversation_query(full, &crate::conversation::WHOLE_BODY)
        );
        assert_eq!(
            conversation_query(continue_body, &crate::conversation::WHOLE_BODY),
            (String::new(), 1)
        );
        Ok(())
    }

    #[test]
    fn malformed_body_has_no_query() {
        assert_eq!(
            conversation_query(b"invalid", &crate::conversation::WHOLE_BODY),
            (String::new(), 0)
        );
    }
}
