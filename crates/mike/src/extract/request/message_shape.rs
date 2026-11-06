use serde_json::Value;

pub(crate) fn messages_ignoring_trailing_system(body: &Value) -> Option<&[Value]> {
    let msgs = body.get("messages").and_then(|v| v.as_array())?;
    let mut end = msgs.len();
    while end > 0 && msgs[end - 1].get("role").and_then(|r| r.as_str()) == Some("system") {
        end -= 1;
    }
    Some(&msgs[..end])
}

pub(super) fn fingerprint_message_count(body: &Value) -> usize {
    messages_ignoring_trailing_system(body)
        .map(<[Value]>::len)
        .unwrap_or(0)
}

pub(crate) fn detect_tool_loop(body: &Value) -> bool {
    use crate::extract::content::has_content_block_type;

    let msgs = match messages_ignoring_trailing_system(body) {
        Some(m) if !m.is_empty() => m,
        _ => return false,
    };

    let len = msgs.len();
    let last = &msgs[len - 1];

    let is_user_tool_result = last.get("role").and_then(|r| r.as_str()) == Some("user")
        && (has_content_block_type(last, "tool_result") || is_raw_json_string_content(last));

    if !is_user_tool_result {
        return false;
    }

    if len == 1 {
        return true;
    }

    let second_last = &msgs[len - 2];
    second_last.get("role").and_then(|r| r.as_str()) == Some("assistant")
        && has_content_block_type(second_last, "tool_use")
}

pub(super) fn is_raw_json_string_content(msg: &Value) -> bool {
    msg.get("content")
        .and_then(|c| c.as_str())
        .map(|s| {
            let t = s.trim_start();
            t.starts_with('{') || t.starts_with('[')
        })
        .unwrap_or(false)
}

#[cfg(test)]
#[path = "message_shape_tests.rs"]
mod tests;
