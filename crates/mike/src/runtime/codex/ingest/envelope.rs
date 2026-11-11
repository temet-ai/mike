use std::collections::BTreeMap;

use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;
use uuid::Uuid;

use super::IngestSource;
use crate::runtime::codex::RpcScope;
use crate::runtime::{RuntimeEventEnvelope, RuntimeEventSource};

pub(super) const SOURCE_KIND: &str = "codex_app_server";
pub(super) const ADAPTER: &str = "mike_codex_ingest";
pub(super) const ADAPTER_VERSION: &str = "1";

pub(super) struct EnvelopeInput<'a> {
    pub namespace_id: Uuid,
    pub source: &'a IngestSource,
    pub parsed: Option<&'a Value>,
    pub scope: Option<&'a RpcScope>,
    pub cursor: String,
    pub received_at: DateTime<Utc>,
}

pub(super) fn envelope(input: EnvelopeInput<'_>) -> RuntimeEventEnvelope {
    let params = input.parsed.and_then(|value| value.get("params"));
    let response = input
        .parsed
        .is_some_and(|value| value.get("method").is_none());
    let (thread, turn, item) = if response {
        (
            input.scope.and_then(|scope| scope.thread_id.clone()),
            input.scope.and_then(|scope| scope.turn_id.clone()),
            None,
        )
    } else {
        (
            provider_id(params, &["/threadId", "/thread/id"]),
            provider_id(params, &["/turnId", "/turn/id"]),
            provider_id(params, &["/itemId", "/item/id"]),
        )
    };
    RuntimeEventEnvelope {
        version: 1,
        ingestion_id: Uuid::new_v4(),
        namespace_id: input.namespace_id,
        source: RuntimeEventSource {
            kind: SOURCE_KIND.into(),
            source_version: Some(input.source.codex_version.clone()),
            adapter: ADAPTER.into(),
            adapter_version: ADAPTER_VERSION.into(),
            runtime_version: Some(input.source.codex_version.clone()),
            fidelity: "original_event_bytes".into(),
            completeness: if response { "rpc_response" } else { "live" }.into(),
            extra: BTreeMap::new(),
        },
        received_at: input.received_at,
        provider_timestamp: input.parsed.and_then(emitted_at),
        provider_session_id: None,
        provider_thread_id: thread,
        provider_turn_id: turn,
        provider_item_id: item,
        provider_event_id: None,
        source_cursor: Some(input.cursor),
        extra: BTreeMap::new(),
    }
}

fn provider_id(params: Option<&Value>, pointers: &[&str]) -> Option<String> {
    let params = params?;
    pointers
        .iter()
        .find_map(|pointer| params.pointer(pointer).and_then(Value::as_str))
        .filter(|text| crate::runtime::validate_text(text, "provider_id").is_ok())
        .map(str::to_owned)
}

fn emitted_at(value: &Value) -> Option<DateTime<Utc>> {
    let millis = value.get("emittedAtMs").and_then(Value::as_i64)?;
    Utc.timestamp_millis_opt(millis).single()
}
