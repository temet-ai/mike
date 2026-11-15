use anyhow::{Context, bail};
use bytes::Bytes;
use serde_json::Value;

use super::scanner::find_case_insensitive;

#[cfg(test)]
pub mod placeholder_tests;

const EXCERPT_CONTEXT_CHARS: usize = 200;

pub struct CorrectionRequest {
    original_body: Bytes,
    pub weasel_response: String,
    kept: String,
    correction_prompt_template: String,
    trigger_word: String,
}

impl CorrectionRequest {
    pub fn new(
        original_body: Bytes,
        weasel_response: String,
        kept: String,
        correction_prompt_template: String,
        trigger_word: String,
    ) -> Self {
        Self {
            original_body,
            weasel_response,
            kept,
            correction_prompt_template,
            trigger_word,
        }
    }

    pub fn build_body(&self) -> anyhow::Result<Bytes> {
        let mut json: Value = serde_json::from_slice(&self.original_body)
            .context("weasel correction: failed to parse original request body as JSON")?;

        let obj = json
            .as_object_mut()
            .context("weasel correction: original request body is not a JSON object")?;

        let messages = obj
            .entry("messages")
            .or_insert(Value::Array(Vec::new()))
            .as_array_mut()
            .context("weasel correction: 'messages' field is not an array")?;

        let kept = self.kept.trim_end();
        if !kept.is_empty() {
            messages.push(serde_json::json!({
                "role": "assistant",
                "content": [{
                    "type": "text",
                    "text": kept
                }]
            }));
        }

        let correction_text = render_prompt(
            &self.correction_prompt_template,
            &self.trigger_word,
            &self.weasel_response,
        )?;
        messages.push(serde_json::json!({
            "role": "user",
            "content": correction_text
        }));

        let bytes = serde_json::to_vec(&json)
            .context("weasel correction: failed to serialize correction request body")?;
        Ok(Bytes::from(bytes))
    }
}

fn render_prompt(template: &str, word: &str, response: &str) -> anyhow::Result<String> {
    let mut rendered = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        rendered.push_str(&rest[..open]);
        let tail = &rest[open + 1..];
        let Some(name) = placeholder_name(tail) else {
            rendered.push('{');
            rest = tail;
            continue;
        };
        rendered.push_str(&substitute(name, word, response)?);
        rest = &tail[name.len() + 1..];
    }
    rendered.push_str(rest);
    Ok(rendered)
}

fn placeholder_name(after_open_brace: &str) -> Option<&str> {
    let close = after_open_brace.find('}')?;
    let name = &after_open_brace[..close];
    let is_name = !name.is_empty() && name.bytes().all(|b| b.is_ascii_lowercase() || b == b'_');
    is_name.then_some(name)
}

fn substitute(name: &str, word: &str, response: &str) -> anyhow::Result<String> {
    match name {
        "word" => Ok(word.to_owned()),
        "excerpt" => Ok(excerpt_around(response, word)),
        other => bail!(
            "weasel correction prompt: unknown placeholder {{{other}}}; supported placeholders are {{word}} and {{excerpt}}"
        ),
    }
}

fn is_sentence_end(c: char) -> bool {
    matches!(c, '.' | '!' | '?' | '\n')
}

fn excerpt_around(response: &str, word: &str) -> String {
    let (start, end) = find_case_insensitive(response, word).unwrap_or((0, 0));
    let before = &response[..start];
    let after = &response[end..];
    let sentence_start = before.rfind(is_sentence_end).map_or(0, |i| i + 1);
    let sentence_end = after
        .find(is_sentence_end)
        .map_or(response.len(), |i| end + i + 1);
    let window_start = before
        .char_indices()
        .rev()
        .nth(EXCERPT_CONTEXT_CHARS - 1)
        .map_or(0, |(i, _)| i);
    let window_end = after
        .char_indices()
        .nth(EXCERPT_CONTEXT_CHARS)
        .map_or(response.len(), |(i, _)| end + i);
    response[sentence_start.max(window_start)..sentence_end.min(window_end)]
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_original_body(messages: serde_json::Value) -> anyhow::Result<Bytes> {
        let body = serde_json::json!({
            "model": "claude-opus-4-5",
            "max_tokens": 1024,
            "messages": messages,
            "system": "You are a helpful assistant."
        });
        Ok(Bytes::from(serde_json::to_vec(&body)?))
    }

    fn messages_of(body: &Bytes) -> anyhow::Result<Vec<Value>> {
        let json: Value = serde_json::from_slice(body)?;
        json["messages"]
            .as_array()
            .cloned()
            .context("messages not an array")
    }

    #[test]
    fn build_body_cut_reply_appends_the_kept_text_and_the_prompt() -> anyhow::Result<()> {
        let original = make_original_body(serde_json::json!([
            {"role": "user", "content": "Hello"}
        ]))?;
        let cr = CorrectionRequest::new(
            original,
            "This is a comprehensive response.".to_string(),
            "This is a".to_string(),
            "You used '{word}', please fix it.".to_string(),
            "comprehensive".to_string(),
        );
        let messages = messages_of(&cr.build_body()?)?;
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(messages[1]["content"][0]["text"], "This is a");
        assert_eq!(messages[2]["role"], "user");
        assert_eq!(
            messages[2]["content"],
            "You used 'comprehensive', please fix it."
        );
        Ok(())
    }

    #[test]
    fn build_body_kept_ending_in_whitespace_is_trimmed_and_the_default_prompt_renders()
    -> anyhow::Result<()> {
        let original = make_original_body(serde_json::json!([
            {"role": "user", "content": "Why did it fail?"}
        ]))?;
        let cr = CorrectionRequest::new(
            original,
            "The database migration failed because of a pre-existing table".to_string(),
            "The database migration failed because of a \n".to_string(),
            crate::features_settings::sections::weasel_intercept::DEFAULT_CORRECTION_PROMPT
                .to_string(),
            "pre-existing".to_string(),
        );
        let messages = messages_of(&cr.build_body()?)?;
        assert_eq!(
            messages[1]["content"][0]["text"],
            "The database migration failed because of a"
        );
        assert_eq!(
            messages[2]["content"],
            "Your reply was cut just before the word 'pre-existing', which is not accepted here. \
             Continue from exactly where it was cut, without repeating any of it and without that word."
        );
        Ok(())
    }

    #[test]
    fn build_body_word_at_the_very_start_adds_no_empty_assistant_turn() -> anyhow::Result<()> {
        let original = make_original_body(serde_json::json!([
            {"role": "user", "content": "Hello"}
        ]))?;
        let cr = CorrectionRequest::new(
            original,
            "Robust answer.".to_string(),
            "  ".to_string(),
            "Fix '{word}'.".to_string(),
            "robust".to_string(),
        );
        let messages = messages_of(&cr.build_body()?)?;
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[1]["content"], "Fix 'robust'.");
        Ok(())
    }

    #[test]
    fn build_body_preserves_model_and_system() -> anyhow::Result<()> {
        let original = make_original_body(serde_json::json!([
            {"role": "user", "content": "Hello"}
        ]))?;
        let cr = CorrectionRequest::new(
            original,
            "weasel text".to_string(),
            "weasel".to_string(),
            "Fix '{word}'.".to_string(),
            "comprehensive".to_string(),
        );
        let body = cr.build_body()?;
        let json: Value = serde_json::from_slice(&body)?;
        assert_eq!(json["model"], "claude-opus-4-5");
        assert_eq!(json["system"], "You are a helpful assistant.");
        assert_eq!(json["max_tokens"], 1024);
        Ok(())
    }

    #[test]
    fn build_body_substitutes_word_in_prompt() -> anyhow::Result<()> {
        let original = make_original_body(serde_json::json!([]))?;
        let cr = CorrectionRequest::new(
            original,
            "text".to_string(),
            "text".to_string(),
            "Do not use '{word}'.".to_string(),
            "robust".to_string(),
        );
        let body = cr.build_body()?;
        let json: Value = serde_json::from_slice(&body)?;
        let messages = json["messages"]
            .as_array()
            .context("messages not an array")?;
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1]["content"], "Do not use 'robust'.");
        Ok(())
    }

    #[test]
    fn build_body_returns_error_on_invalid_json() {
        let cr = CorrectionRequest::new(
            Bytes::from_static(b"not json"),
            "text".to_string(),
            "text".to_string(),
            "Fix '{word}'.".to_string(),
            "word".to_string(),
        );
        assert!(cr.build_body().is_err());
    }
}
