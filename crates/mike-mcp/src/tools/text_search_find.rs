use std::time::{Duration, Instant};

use rmcp::model::{ErrorCode, ErrorData as RmcpError};
use serde_json::{Value, json};

use super::text_search::{FindTextArgs, find_text_query};
use crate::client::InspectClient;
use crate::error::McpError;

pub(super) const FIND_TEXT_SETTLE_BUDGET: Duration = Duration::from_secs(30);

pub(super) async fn find_text_settled(
    client: &InspectClient,
    mut args: FindTextArgs,
    budget: Duration,
) -> Result<Value, RmcpError> {
    let started = Instant::now();
    loop {
        let value = client
            .get_json("/api/body/find", &find_text_query(&args))
            .await
            .map_err(McpError::into_rmcp)?;
        if !is_empty_and_incomplete(&value) {
            return Ok(value);
        }
        let next = value.get("next_cursor").and_then(Value::as_u64);
        match next {
            Some(cursor) if started.elapsed() < budget && args.cursor != Some(cursor) => {
                args.cursor = Some(cursor);
            }
            _ => return Err(incomplete_error(&args, &value, next)),
        }
    }
}

fn is_empty_and_incomplete(value: &Value) -> bool {
    let no_matches = value
        .get("matches")
        .and_then(Value::as_array)
        .is_some_and(Vec::is_empty);
    no_matches && value.get("scan_complete") == Some(&Value::Bool(false))
}

fn incomplete_error(args: &FindTextArgs, value: &Value, cursor: Option<u64>) -> RmcpError {
    let scanned = value.get("requests_scanned").and_then(Value::as_u64);
    let total = value.get("requests_total").and_then(Value::as_u64);
    let progress = match (scanned, total) {
        (Some(scanned), Some(total)) => format!("{scanned} of {total} requests"),
        _ => "part of the requests".to_owned(),
    };
    let resume = match cursor {
        Some(cursor) => format!("call find_text again with cursor={cursor} to continue"),
        None => "the server gave no cursor to continue from".to_owned(),
    };
    let message = format!(
        "find_text scanned {progress} of the scope for {needle:?} and found no match before its \
         time ran out, so the absence is not established; {resume}",
        needle = args.needle,
    );
    let data = json!({
        "kind": "find_text_incomplete",
        "needle": args.needle,
        "session_id": args.session_id,
        "requests_scanned": scanned,
        "requests_total": total,
        "next_cursor": cursor,
    });
    RmcpError::new(ErrorCode::INTERNAL_ERROR, message, Some(data))
}

#[cfg(test)]
#[path = "text_search_find_tests.rs"]
mod text_search_find_tests;
