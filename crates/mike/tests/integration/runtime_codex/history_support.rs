use std::collections::BTreeMap;

use chrono::DateTime;
use mike::runtime::codex::RequestId;
use mike::runtime::codex::history::{
    HistoryCursorLedger, HistoryCursorVisit, HistoryIdentity, HistoryQuery, HistoryRequest,
};
use mike::runtime::{RuntimeEventEnvelope, RuntimeEventSource, RuntimeObservation};
use serde_json::{Value, json};
use uuid::Uuid;

pub(super) fn identity() -> anyhow::Result<HistoryIdentity> {
    Ok(HistoryIdentity::new(
        Uuid::from_u128(900),
        Uuid::from_u128(42),
        "Thread:保留 ".into(),
    )?)
}

pub(super) fn turns(cursor: Option<&str>) -> anyhow::Result<HistoryRequest> {
    Ok(
        HistoryQuery::turns(identity()?, 2, cursor.map(str::to_owned))?
            .request(RequestId::Integer(7)),
    )
}

pub(super) fn items(cursor: Option<&str>) -> anyhow::Result<HistoryRequest> {
    Ok(
        HistoryQuery::items(identity()?, "Turn:A".into(), 2, cursor.map(str::to_owned))?
            .request(RequestId::Integer(7)),
    )
}

pub(super) fn frame(result: Value) -> anyhow::Result<RuntimeObservation> {
    raw(&serde_json::to_vec(&json!({"id":7,"result":result}))?)
}

pub(super) fn raw(bytes: &[u8]) -> anyhow::Result<RuntimeObservation> {
    Ok(RuntimeObservation::new(
        RuntimeEventEnvelope {
            version: 1,
            ingestion_id: Uuid::from_u128(43),
            namespace_id: Uuid::from_u128(42),
            source: RuntimeEventSource {
                kind: "synthetic_history_fixture".into(),
                source_version: Some("official-schema-0.159.2".into()),
                adapter: "offline".into(),
                adapter_version: "1".into(),
                runtime_version: Some("0.159.2".into()),
                fidelity: "original_event_bytes".into(),
                completeness: "synthetic".into(),
                extra: BTreeMap::new(),
            },
            received_at: DateTime::from_timestamp(1_700_000_000, 0)
                .ok_or_else(|| anyhow::anyhow!("invalid fixture time"))?,
            provider_timestamp: None,
            provider_session_id: None,
            provider_thread_id: None,
            provider_turn_id: None,
            provider_item_id: None,
            provider_event_id: None,
            source_cursor: None,
            extra: BTreeMap::new(),
        },
        bytes.to_vec(),
    )?)
}

#[derive(Default)]
pub(super) struct Ledger {
    pub committed: Vec<(HistoryQuery, String)>,
    pub unavailable: bool,
}

impl HistoryCursorLedger for Ledger {
    fn lookup(&self, query: &HistoryQuery, cursor: &str) -> HistoryCursorVisit {
        if self.unavailable {
            return HistoryCursorVisit::Unknown;
        }
        if self.committed.iter().any(|(scope, saved)| {
            scope.identity() == query.identity()
                && scope.scope() == query.scope()
                && scope.limit() == query.limit()
                && saved == cursor
        }) {
            HistoryCursorVisit::Committed
        } else {
            HistoryCursorVisit::NotVisited
        }
    }
}
