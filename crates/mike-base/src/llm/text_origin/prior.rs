use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Prior {
    pub user_message: String,
    pub assistant_reply: String,
}

fn text_of(message: &Value) -> String {
    match message.get("content") {
        Some(Value::String(text)) => text.trim().to_owned(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .filter(|text| !text.trim_start().starts_with("<system-reminder>"))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_owned(),
        _ => String::new(),
    }
}

fn role_is(message: &Value, role: &str) -> bool {
    message.get("role").and_then(Value::as_str) == Some(role)
}

pub fn prior_context(body: &[u8]) -> Prior {
    let Ok(parsed) = serde_json::from_slice::<Value>(body) else {
        return Prior::default();
    };
    let Some(messages) = parsed.get("messages").and_then(Value::as_array) else {
        return Prior::default();
    };
    let users: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| role_is(m, "user") && !text_of(m).is_empty())
        .map(|(i, _)| i)
        .collect();
    let Some(&current) = users.last() else {
        return Prior::default();
    };
    let user_message = users
        .len()
        .checked_sub(2)
        .map(|at| text_of(&messages[users[at]]))
        .unwrap_or_default();
    let assistant_reply = messages[..current]
        .iter()
        .rev()
        .filter(|m| role_is(m, "assistant"))
        .map(text_of)
        .find(|text| !text.is_empty())
        .unwrap_or_default();
    Prior {
        user_message,
        assistant_reply,
    }
}
