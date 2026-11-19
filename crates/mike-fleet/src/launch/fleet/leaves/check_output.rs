use chrono::{Duration, Utc};
use serde_json::Value;

use super::judge::CitedReceipt;
use crate::llm::decisions::text::tail_chars;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::TreeCheckRow;
use crate::store::persistence::fleet_repo::FleetPersist as _;

pub const OUTPUT_CHARS: usize = 2_000;
const LOOK_BACK_HOURS: i64 = 24;
const MAX_CITED: usize = 4;

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

pub fn cited_receipt_ids(text: &str) -> Vec<i64> {
    let lower = text.to_ascii_lowercase();
    let mut ids = Vec::new();
    let mut rest = lower.as_str();
    while let Some(at) = rest.find("receipt") {
        rest = &rest[at + "receipt".len()..];
        let digits: String = rest
            .trim_start_matches(|c: char| c == 's' || c == '#' || c == ':' || c.is_whitespace())
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if let Ok(id) = digits.parse::<i64>()
            && !ids.contains(&id)
        {
            ids.push(id);
        }
    }
    ids
}

pub fn cited_receipt(row: &TreeCheckRow) -> CitedReceipt {
    CitedReceipt {
        receipt: row.id,
        command: row.command.clone(),
        exit_code: row.exit_code,
        output_tail: tail_chars(row.output.trim(), OUTPUT_CHARS).to_owned(),
    }
}

pub async fn cited_receipts(persist: &PersistHandle, run: &str, cited: &str) -> Vec<CitedReceipt> {
    let mut out = Vec::new();
    for id in cited_receipt_ids(cited).into_iter().take(MAX_CITED) {
        match persist.fleet().tree_check(id).await {
            Ok(Some(row)) if row.run_id == run => out.push(cited_receipt(&row)),
            Ok(_) => {}
            Err(err) => {
                tracing::warn!(%run, id, error = %err, "leaf judge: could not read a cited receipt");
            }
        }
    }
    out
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
