use std::collections::HashSet;

use serde_json::Value;

use super::*;

const SEARCH_TOOL_SUFFIXES: [&str; 5] = [
    "find_text",
    "search_session_content",
    "search_project_knowledge",
    "recall_similar_turns",
    "recall_project_context",
];

pub(super) fn search_tool_suffixes() -> Vec<String> {
    SEARCH_TOOL_SUFFIXES
        .iter()
        .map(|suffix| (*suffix).to_owned())
        .collect()
}

pub(super) fn is_search_tool_name(name: &str) -> bool {
    SEARCH_TOOL_SUFFIXES
        .iter()
        .any(|suffix| name.ends_with(suffix))
}

#[derive(Debug, Default)]
pub(super) struct OwnSearchCalls {
    tool_use_ids: HashSet<String>,
}

impl OwnSearchCalls {
    pub(super) fn claims_message(
        &mut self,
        message: &Value,
        needle: &str,
        case_sensitive: bool,
    ) -> bool {
        let Some(blocks) = message.get("content").and_then(Value::as_array) else {
            return false;
        };
        let mut claimed = false;
        for block in blocks {
            claimed |= match block.get("type").and_then(Value::as_str) {
                Some("tool_use") => self.record_call(block, needle, case_sensitive),
                Some("tool_result") => self.is_result_of_recorded_call(block),
                _ => false,
            };
        }
        claimed
    }

    fn record_call(&mut self, block: &Value, needle: &str, case_sensitive: bool) -> bool {
        let is_search = block
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(is_search_tool_name);
        if !is_search || !input_contains(block.get("input"), needle, case_sensitive) {
            return false;
        }
        if let Some(id) = block.get("id").and_then(Value::as_str) {
            self.tool_use_ids.insert(id.to_owned());
        }
        true
    }

    fn is_result_of_recorded_call(&self, block: &Value) -> bool {
        block
            .get("tool_use_id")
            .and_then(Value::as_str)
            .is_some_and(|id| self.tool_use_ids.contains(id))
    }
}

fn input_contains(input: Option<&Value>, needle: &str, case_sensitive: bool) -> bool {
    let Some(input) = input else {
        return false;
    };
    let text = input.to_string();
    !crate::web_base::body_search::match_offsets(&text, needle, case_sensitive).is_empty()
}

pub(super) async fn own_search_request_ids(
    handle: &crate::store::persistence::PersistHandle,
    request_ids: &[i64],
    needle: &str,
    case_sensitive: bool,
) -> Result<HashSet<i64>, (StatusCode, Json<FindTextErrorBody>)> {
    let ids = handle
        .find_own_search_request_ids(request_ids, needle, case_sensitive, &search_tool_suffixes())
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "find_text: own-search request lookup failed");
            find_text_error(StatusCode::INTERNAL_SERVER_ERROR, "lookup failed")
        })?;
    Ok(ids.into_iter().collect())
}

#[cfg(test)]
#[path = "search_own_tests.rs"]
mod search_own_tests;
