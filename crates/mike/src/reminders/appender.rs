use serde_json::{Value, json};

use crate::compress::manifest::ByteRange;
use crate::store::models::RequestId;

pub(crate) fn append_block_to_last_user_message(
    body_bytes: &[u8],
    reminder_text: &str,
    request_id: RequestId,
) -> Option<Vec<u8>> {
    if reminder_text.trim().is_empty() {
        return None;
    }
    let value: Value = serde_json::from_slice(body_bytes).ok()?;
    let obj = value.as_object()?;
    let messages = obj.get("messages")?.as_array()?;

    let last_user_idx = messages
        .iter()
        .rposition(|m| m.get("role").and_then(Value::as_str) == Some("user"))?;

    let last_assistant_idx = messages
        .iter()
        .rposition(|m| m.get("role").and_then(Value::as_str) == Some("assistant"));
    if last_assistant_idx.is_some_and(|a| a > last_user_idx) {
        return None;
    }

    let content = messages[last_user_idx].get("content")?;
    let content_shape = match content {
        Value::Array(items) => ContentKind::Blocks {
            empty: items.is_empty(),
        },
        Value::String(_) => ContentKind::PlainString,
        _ => return None,
    };

    let block_bytes = serde_json::to_vec(&json!({
        "type": "text",
        "text": reminder_text,
    }))
    .ok()?;

    if let Some(spliced) =
        splice_block_into_content(body_bytes, last_user_idx, content_shape, &block_bytes)
    {
        return Some(spliced);
    }

    fallback_whole_body_reserialize(value, last_user_idx, reminder_text, request_id)
}

#[derive(Clone, Copy)]
enum ContentKind {
    Blocks { empty: bool },
    PlainString,
}

fn splice_into_plain_string(
    body_bytes: &[u8],
    content_span: ByteRange,
    block_bytes: &[u8],
) -> Option<Vec<u8>> {
    let original = body_bytes.get(content_span.start..content_span.end)?;
    if original.first() != Some(&b'"') {
        return None;
    }
    let mut out = Vec::with_capacity(body_bytes.len() + block_bytes.len() + 32);
    out.extend_from_slice(body_bytes.get(..content_span.start)?);
    out.extend_from_slice(br#"[{"type":"text","text":"#);
    out.extend_from_slice(original);
    out.extend_from_slice(b"},");
    out.extend_from_slice(block_bytes);
    out.push(b']');
    out.extend_from_slice(body_bytes.get(content_span.end..)?);
    Some(out)
}

fn fallback_whole_body_reserialize(
    mut value: Value,
    last_user_idx: usize,
    reminder_text: &str,
    request_id: RequestId,
) -> Option<Vec<u8>> {
    tracing::warn!(
        %request_id,
        "append_block_to_last_user_message: could not locate the last user \
         message's content-array byte span in the original body — falling \
         back to a whole-body reserialize, which resorts every object's \
         keys in `messages` and breaks Anthropic's prompt-cache key for \
         this request"
    );
    let content = value
        .get_mut("messages")?
        .get_mut(last_user_idx)?
        .get_mut("content")?;
    if let Value::String(original) = content {
        *content = json!([{"type": "text", "text": original.clone()}]);
    }
    content.as_array_mut()?.push(json!({
        "type": "text",
        "text": reminder_text,
    }));
    serde_json::to_vec(&value).ok()
}

fn splice_block_into_content(
    body_bytes: &[u8],
    target_idx: usize,
    content_shape: ContentKind,
    block_bytes: &[u8],
) -> Option<Vec<u8>> {
    let content_span = locate_content_span(body_bytes, target_idx)?;
    let content_is_empty = match content_shape {
        ContentKind::Blocks { empty } => empty,
        ContentKind::PlainString => {
            return splice_into_plain_string(body_bytes, content_span, block_bytes);
        }
    };
    let closing_bracket = content_span.end.checked_sub(1)?;
    if body_bytes.get(closing_bracket) != Some(&b']') {
        return None;
    }

    let mut out = Vec::with_capacity(body_bytes.len() + block_bytes.len() + 1);
    out.extend_from_slice(body_bytes.get(..closing_bracket)?);
    if !content_is_empty {
        out.push(b',');
    }
    out.extend_from_slice(block_bytes);
    out.extend_from_slice(body_bytes.get(closing_bracket..)?);
    Some(out)
}

fn locate_content_span(body_bytes: &[u8], target_idx: usize) -> Option<ByteRange> {
    #[derive(serde::Deserialize)]
    struct MessagesShape<'a> {
        #[serde(borrow, default)]
        messages: Vec<&'a serde_json::value::RawValue>,
    }
    #[derive(serde::Deserialize)]
    struct ContentShape<'a> {
        #[serde(borrow)]
        content: &'a serde_json::value::RawValue,
    }

    let parsed: MessagesShape<'_> = serde_json::from_slice(body_bytes).ok()?;
    let msg_text: &str = parsed.messages.get(target_idx)?.get();
    let content_shape: ContentShape<'_> = serde_json::from_str(msg_text).ok()?;
    let content_text: &str = content_shape.content.get();

    let base = body_bytes.as_ptr() as usize;
    let start = (content_text.as_ptr() as usize).checked_sub(base)?;
    let end = start.checked_add(content_text.len())?;
    (end <= body_bytes.len()).then_some(ByteRange { start, end })
}

pub(crate) fn payload_present_in_last_user_message(body_bytes: &[u8], reminder_text: &str) -> bool {
    let Ok(value) = serde_json::from_slice::<Value>(body_bytes) else {
        return false;
    };
    let Some(messages) = value.get("messages").and_then(Value::as_array) else {
        return false;
    };
    let Some(last_user) = messages
        .iter()
        .rev()
        .find(|m| m.get("role").and_then(Value::as_str) == Some("user"))
    else {
        return false;
    };
    let Some(content) = last_user.get("content").and_then(Value::as_array) else {
        return false;
    };
    content.iter().any(|block| {
        block.get("type").and_then(Value::as_str) == Some("text")
            && block.get("text").and_then(Value::as_str) == Some(reminder_text)
    })
}
