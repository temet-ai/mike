use std::collections::HashSet;

use serde_json::{Value, json};

use super::record::{Draft, spawn_record};
use crate::config::AppState;

const CONVERSATION_TOOL: &str = "search_session_content";
const TEXT_TOOL: &str = "find_text";
pub(super) const MAX_TEXT_REFERENCES: usize = 50;

pub(crate) fn record_conversation_search(
    state: &AppState,
    caller: Option<&str>,
    query: &str,
    sessions: &[(String, u64)],
) {
    if let Some(caller) = caller {
        spawn_record(state, Some(caller), conversation_draft(query, sessions));
    }
}

pub(crate) fn record_text_search(
    state: &AppState,
    caller: Option<&str>,
    needle: &str,
    request_ids: impl IntoIterator<Item = u64>,
) {
    if let Some(caller) = caller {
        spawn_record(state, Some(caller), text_draft(needle, request_ids));
    }
}

pub(super) fn conversation_draft(query: &str, sessions: &[(String, u64)]) -> Draft {
    let returned = sessions
        .iter()
        .map(|(session, matches)| json!({"reference": format!("session:{session}"), "matches": matches}))
        .collect();
    Draft {
        tool: CONVERSATION_TOOL,
        query: query.to_owned(),
        kind_filter: None,
        returned: Value::Array(returned),
        rerank_state: "off",
    }
}

pub(super) fn text_draft(needle: &str, request_ids: impl IntoIterator<Item = u64>) -> Draft {
    let mut seen = HashSet::new();
    let returned = request_ids
        .into_iter()
        .filter(|id| seen.insert(*id))
        .take(MAX_TEXT_REFERENCES)
        .map(|id| json!({"reference": format!("request:{id}")}))
        .collect();
    Draft {
        tool: TEXT_TOOL,
        query: needle.to_owned(),
        kind_filter: None,
        returned: Value::Array(returned),
        rerank_state: "off",
    }
}
