use std::collections::BTreeMap;

use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{
    IntakeContext, ItemObservation, ProjectionEffect, RequestId, RpcScope, decode_observation,
};
use crate::runtime::{RuntimeEventEnvelope, RuntimeEventSource, RuntimeObservation};

pub(super) fn envelope() -> RuntimeEventEnvelope {
    RuntimeEventEnvelope {
        version: 1,
        ingestion_id: Uuid::new_v4(),
        namespace_id: Uuid::from_u128(42),
        source: RuntimeEventSource {
            kind: "synthetic_app_server_fixture".into(),
            source_version: Some("official-schema-0.159.2".into()),
            adapter: "codex_projection_test".into(),
            adapter_version: "1".into(),
            runtime_version: Some("0.159.2".into()),
            fidelity: "original_event_bytes".into(),
            completeness: "synthetic".into(),
            extra: BTreeMap::new(),
        },
        received_at: Utc::now(),
        provider_timestamp: None,
        provider_session_id: None,
        provider_thread_id: None,
        provider_turn_id: None,
        provider_item_id: None,
        provider_event_id: None,
        source_cursor: None,
        extra: BTreeMap::new(),
    }
}

pub(super) fn observation(value: Value) -> anyhow::Result<RuntimeObservation> {
    Ok(RuntimeObservation::new(
        envelope(),
        serde_json::to_vec(&value)?,
    )?)
}

pub(super) fn notification(method: &str, params: Value) -> anyhow::Result<RuntimeObservation> {
    observation(json!({"method":method,"params":params}))
}

pub(super) fn item_event(method: &str, item: Value) -> anyhow::Result<ItemObservation> {
    let event = notification(
        method,
        json!({
            "threadId":"Thread:Case/保留 ","turnId":"Turn:A","item":item,
            "startedAtMs":1720000000123i64
        }),
    )?;
    let mut event = event;
    if method == "item/completed" {
        let mut payload = event
            .parsed_payload
            .clone()
            .ok_or_else(|| anyhow::anyhow!("missing fixture JSON"))?;
        payload["params"]["completedAtMs"] = json!(1720000004567i64);
        event = observation(payload)?;
    }
    first_item(&event, None)
}

pub(super) fn item_delta(text: &str) -> anyhow::Result<ItemObservation> {
    let event = notification(
        "item/agentMessage/delta",
        json!({
            "threadId":"Thread:Case/保留 ","turnId":"Turn:A","itemId":"Item:1","delta":text
        }),
    )?;
    first_item(&event, None)
}

pub(super) fn first_item(
    event: &RuntimeObservation,
    scope: Option<&RpcScope>,
) -> anyhow::Result<ItemObservation> {
    let batch = decode_observation(event, IntakeContext::new(scope));
    match batch.effects.into_iter().next() {
        Some(ProjectionEffect::Item(item)) => Ok(*item),
        _ => anyhow::bail!("synthetic fixture did not produce an item"),
    }
}

pub(super) fn scope(method: &str) -> RpcScope {
    RpcScope {
        request_id: RequestId::String("Request:Mixed/Case".into()),
        method: method.into(),
        thread_id: Some("Thread:Case/保留 ".into()),
        turn_id: Some("Turn:A".into()),
    }
}
