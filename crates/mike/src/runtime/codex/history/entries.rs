use serde_json::Value;

use super::{HistoryError, HistoryErrorKind, HistoryRequest};
use crate::runtime::codex::protocol::field;
use crate::runtime::codex::{Evidence, Field, ItemCategory};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryMetadata<'a> {
    pub thread_id: &'a str,
    pub history_mode: Field<String>,
    pub ephemeral: Field<bool>,
    pub evidence: Evidence,
    pub provider_payload: &'a Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryTurn<'a> {
    pub turn_id: &'a str,
    pub items_view: Field<String>,
    pub status: Field<String>,
    pub started_at_seconds: Field<i64>,
    pub completed_at_seconds: Field<i64>,
    pub evidence: Evidence,
    pub provider_payload: &'a Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryItem<'a> {
    pub turn_id: &'a str,
    pub item_id: &'a str,
    pub provider_type: &'a str,
    pub category: ItemCategory,
    pub started_at_ms: Field<i64>,
    pub completed_at_ms: Field<i64>,
    pub evidence: Evidence,
    pub provider_payload: &'a Value,
    pub entry_payload: &'a Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryEntry<'a> {
    Turn(HistoryTurn<'a>),
    Item(HistoryItem<'a>),
}

pub(super) fn metadata<'a>(
    value: &'a Value,
    request: &HistoryRequest,
    evidence: Evidence,
) -> Result<HistoryMetadata<'a>, HistoryError> {
    let thread_id = identifier(value, "id", &evidence.json_pointer)?;
    if thread_id != request.query().identity().thread_id() {
        return Err(HistoryError::new(
            HistoryErrorKind::ConflictingScope,
            "/result/thread/id",
        ));
    }
    Ok(HistoryMetadata {
        thread_id,
        history_mode: field(value, "historyMode"),
        ephemeral: field(value, "ephemeral"),
        evidence,
        provider_payload: value,
    })
}

pub(super) fn turn<'a>(
    value: &'a Value,
    request: &HistoryRequest,
    evidence: Evidence,
) -> Result<HistoryEntry<'a>, HistoryError> {
    check_optional_scope(
        value,
        "threadId",
        Some(request.query().identity().thread_id()),
        &evidence.json_pointer,
    )?;
    Ok(HistoryEntry::Turn(HistoryTurn {
        turn_id: identifier(value, "id", &evidence.json_pointer)?,
        items_view: field(value, "itemsView"),
        status: field(value, "status"),
        started_at_seconds: field(value, "startedAt"),
        completed_at_seconds: field(value, "completedAt"),
        evidence,
        provider_payload: value,
    }))
}

pub(super) fn item<'a>(
    entry: &'a Value,
    request: &HistoryRequest,
    mut evidence: Evidence,
) -> Result<HistoryEntry<'a>, HistoryError> {
    let pointer = &evidence.json_pointer;
    check_optional_scope(
        entry,
        "threadId",
        Some(request.query().identity().thread_id()),
        pointer,
    )?;
    let turn_id = identifier(entry, "turnId", pointer)?;
    if Some(turn_id) != request.query().turn_id() {
        return Err(HistoryError::new(
            HistoryErrorKind::ConflictingScope,
            format!("{pointer}/turnId"),
        ));
    }
    let value = entry.get("item").ok_or_else(|| {
        HistoryError::new(HistoryErrorKind::InvalidData, format!("{pointer}/item"))
    })?;
    evidence.json_pointer.push_str("/item");
    let provider_type = identifier(value, "type", &evidence.json_pointer)?;
    Ok(HistoryEntry::Item(HistoryItem {
        turn_id,
        item_id: identifier(value, "id", &evidence.json_pointer)?,
        provider_type,
        category: ItemCategory::from_provider_type(provider_type),
        started_at_ms: field(entry, "startedAtMs"),
        completed_at_ms: field(entry, "completedAtMs"),
        evidence,
        provider_payload: value,
        entry_payload: entry,
    }))
}

pub(super) fn identifier<'a>(
    value: &'a Value,
    key: &str,
    pointer: &str,
) -> Result<&'a str, HistoryError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty() && !text.contains('\0'))
        .ok_or_else(|| {
            HistoryError::new(
                HistoryErrorKind::InvalidIdentifier,
                format!("{pointer}/{key}"),
            )
        })
}

pub(super) fn check_optional_scope(
    value: &Value,
    key: &str,
    expected: Option<&str>,
    pointer: &str,
) -> Result<(), HistoryError> {
    if value.get(key).is_some() && Some(identifier(value, key, pointer)?) != expected {
        return Err(HistoryError::new(
            HistoryErrorKind::ConflictingScope,
            format!("{pointer}/{key}"),
        ));
    }
    Ok(())
}
