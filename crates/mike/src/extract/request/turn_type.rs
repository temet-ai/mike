use std::fmt;

use serde_json::Value;

use super::message_shape::{
    detect_tool_loop, is_raw_json_string_content, messages_ignoring_trailing_system,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnType {
    UserPrompt,
    ToolLoop,
    Continuation,
}

impl fmt::Display for TurnType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UserPrompt => write!(f, "user_prompt"),
            Self::ToolLoop => write!(f, "tool_loop"),
            Self::Continuation => write!(f, "continuation"),
        }
    }
}

pub(crate) fn extract_turn_type(body: &Value) -> TurnType {
    if detect_tool_loop(body) {
        return TurnType::ToolLoop;
    }

    if let Some(msgs) = messages_ignoring_trailing_system(body)
        && msgs.len() == 1
    {
        let msg = &msgs[0];
        if msg.get("role").and_then(|r| r.as_str()) == Some("user")
            && is_raw_json_string_content(msg)
        {
            return TurnType::ToolLoop;
        }
    }

    let has_user = body
        .get("messages")
        .and_then(|v| v.as_array())
        .map(|msgs| {
            msgs.iter()
                .any(|msg| msg.get("role").and_then(|r| r.as_str()) == Some("user"))
        })
        .unwrap_or(false);
    if has_user {
        TurnType::UserPrompt
    } else {
        TurnType::Continuation
    }
}

#[cfg(test)]
#[path = "turn_type_tests.rs"]
mod tests;
