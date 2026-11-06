use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use crate::dossier::build::assemble::parse_spans;
use crate::dossier::build::ended_on::{EndedOn, classify, non_sidecar_text};
use crate::store::persistence::dossier_entries::{
    BackingItemRow, EntryKind, NewDossierEntry, PROMISES_VERSION, PromiseSourceRow, SourceRef,
};

pub fn promise_span(source: &PromiseSourceRow) -> Option<(usize, usize)> {
    let spans = parse_spans(&source.text_spans);
    let ended = if spans.is_empty() {
        classify(&source.final_text)
    } else {
        non_sidecar_text(&source.final_text, &spans).classify()
    };
    if ended.class == EndedOn::PromiseOrWait {
        ended.span
    } else {
        None
    }
}

pub fn has_registry_coverage(
    effective_at: DateTime<Utc>,
    registry_first: Option<DateTime<Utc>>,
) -> bool {
    registry_first.is_some_and(|first| effective_at >= first)
}

fn backing_at(backing: &[BackingItemRow], session_id: i64, at: DateTime<Utc>) -> Vec<Value> {
    backing
        .iter()
        .filter(|item| {
            item.session_id == session_id
                && item.launched_at <= at
                && item.finished_at.is_none_or(|finished| finished > at)
        })
        .map(|item| {
            json!({
                "work_item_id": item.work_item_id,
                "kind": item.kind,
                "task_id": item.task_id,
                "owner_agent_id": item.owner_agent_id,
                "description": item.description,
            })
        })
        .collect()
}

pub fn open_entry(
    source: &PromiseSourceRow,
    backing: &[BackingItemRow],
    registry_first: Option<DateTime<Utc>>,
) -> NewDossierEntry {
    let effective_at = source.stop_completed_at.unwrap_or(source.closed_at);
    let coverage = has_registry_coverage(effective_at, registry_first);
    let backing_value = if coverage {
        Value::Array(backing_at(backing, source.session_id, effective_at))
    } else {
        Value::Null
    };
    let source_ref = source.stop_request_id.map_or_else(
        || SourceRef::operator_turn(source.turn_id),
        SourceRef::request,
    );
    NewDossierEntry {
        session_id: source.session_id,
        operator_turn_id: Some(source.turn_id),
        kind: EntryKind::Promise.code(),
        data: json!({
            "span": promise_span(source).map(|(start, end)| vec![start, end]),
            "backing": backing_value,
            "registry_coverage": coverage,
            "session_class": source.session_class,
        }),
        source_ref: source_ref.to_string(),
        dedup_key: format!("turn:{}", source.turn_id),
        extractor_version: PROMISES_VERSION,
        effective_at,
    }
}

pub fn open_entries(
    sources: &[PromiseSourceRow],
    backing: &[BackingItemRow],
    registry_first: Option<DateTime<Utc>>,
) -> Vec<NewDossierEntry> {
    sources
        .iter()
        .map(|source| open_entry(source, backing, registry_first))
        .collect()
}
