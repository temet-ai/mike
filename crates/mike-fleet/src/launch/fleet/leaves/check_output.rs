use chrono::{Duration, Utc};
use serde_json::Value;

use crate::llm::decisions::text::tail_chars;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet_repo::FleetPersist as _;

pub const OUTPUT_CHARS: usize = 2_000;
const LOOK_BACK_HOURS: i64 = 24;

pub fn result_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| match b {
                Value::String(text) => Some(text.as_str()),
                other => other.get("text").and_then(Value::as_str),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

pub fn excerpt(content: &Value) -> String {
    tail_chars(result_text(content).trim(), OUTPUT_CHARS).to_owned()
}

pub async fn exercised_output(persist: &PersistHandle, run: &str, command: &str) -> String {
    if command.trim().is_empty() {
        return String::new();
    }
    let since = Utc::now() - Duration::hours(LOOK_BACK_HOURS);
    match persist
        .fleet()
        .tree_results(run, Some(command.trim()), since, 1)
        .await
    {
        Ok(rows) => rows
            .first()
            .and_then(|row| row.content.as_ref())
            .map(excerpt)
            .unwrap_or_default(),
        Err(err) => {
            tracing::warn!(%run, error = %err, "leaf judge: could not read the exercised command's output");
            String::new()
        }
    }
}
