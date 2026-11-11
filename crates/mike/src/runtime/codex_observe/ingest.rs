use std::collections::BTreeMap;

use chrono::Utc;
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

use super::apply::{self, Delivery};
use super::hook::HookPayload;
use super::otel;
use super::writes::{self, StoreError};
use super::{ADAPTER, ADAPTER_VERSION};
use crate::runtime::{
    RuntimeEventEnvelope, RuntimeEventSource, RuntimeKind, RuntimeNamespace, RuntimeObservation,
};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::runtime::RuntimeWriteOutcome;

const NAMESPACE_SEED: Uuid = Uuid::from_u128(0x6d69_6b65_636f_6465_786f_6273_6572_7665);

#[derive(Debug, Clone)]
pub(crate) struct ObserveSource {
    pub label: String,
    pub codex_version: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub(crate) struct IngestReport {
    pub namespace_id: Uuid,
    pub ingestion_id: Uuid,
    pub replayed: bool,
    pub facts: usize,
    pub control: &'static str,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum IngestError {
    #[error("{0}")]
    BadInput(String),
    #[error("{0}")]
    Store(String),
}

impl From<StoreError> for IngestError {
    fn from(error: StoreError) -> Self {
        Self::Store(error.to_string())
    }
}

impl From<crate::store::persistence::PersistError> for IngestError {
    fn from(error: crate::store::persistence::PersistError) -> Self {
        Self::Store(error.to_string())
    }
}

pub(crate) fn namespace_for(source: &ObserveSource) -> RuntimeNamespace {
    let durable_key = format!("codex-observed:{}", source.label);
    RuntimeNamespace {
        id: Uuid::new_v5(&NAMESPACE_SEED, durable_key.as_bytes()),
        runtime: RuntimeKind::Codex,
        durable_key,
    }
}

pub(crate) fn otel_ingestion_id(namespace: &RuntimeNamespace, signal: &str, raw: &[u8]) -> Uuid {
    let mut name = signal.as_bytes().to_vec();
    name.push(0);
    name.extend_from_slice(raw);
    Uuid::new_v5(&namespace.id, &name)
}

struct EventIds {
    session: Option<String>,
    thread: Option<String>,
    turn: Option<String>,
    item: Option<String>,
}

fn envelope(
    namespace: &RuntimeNamespace,
    ingestion_id: Uuid,
    source: &ObserveSource,
    kind: &str,
    ids: EventIds,
    extra: BTreeMap<String, Value>,
) -> RuntimeEventEnvelope {
    let non_empty = |value: Option<String>| {
        value.filter(|text| !text.trim().is_empty() && !text.contains('\0'))
    };
    RuntimeEventEnvelope {
        version: 1,
        ingestion_id,
        namespace_id: namespace.id,
        source: RuntimeEventSource {
            kind: kind.to_owned(),
            source_version: source.codex_version.clone(),
            adapter: ADAPTER.to_owned(),
            adapter_version: ADAPTER_VERSION.to_owned(),
            runtime_version: source.codex_version.clone(),
            fidelity: if kind == "codex_hook" {
                "hook_payload"
            } else {
                "telemetry_summary"
            }
            .to_owned(),
            completeness: if kind == "codex_hook" {
                "event_fields_only"
            } else {
                "snippets"
            }
            .to_owned(),
            extra: BTreeMap::from([("observed".to_owned(), json!("unmanaged"))]),
        },
        received_at: Utc::now(),
        provider_timestamp: None,
        provider_session_id: non_empty(ids.session),
        provider_thread_id: non_empty(ids.thread),
        provider_turn_id: non_empty(ids.turn),
        provider_item_id: non_empty(ids.item),
        provider_event_id: None,
        source_cursor: None,
        extra,
    }
}

async fn record<'p>(
    persist: &'p PersistHandle,
    namespace: &RuntimeNamespace,
    envelope: RuntimeEventEnvelope,
    raw: &[u8],
) -> Result<(Delivery<'p>, bool), IngestError> {
    persist.register_runtime_namespace(namespace).await?;
    let at = envelope.received_at;
    let ingestion_id = envelope.ingestion_id;
    let observation = RuntimeObservation::new(envelope, raw.to_vec())
        .map_err(|error| IngestError::BadInput(error.to_string()))?;
    let outcome = persist.append_runtime_observation(&observation).await?;
    let delivery = Delivery {
        pool: persist.pool(),
        namespace_id: namespace.id,
        ingestion_id,
        at,
    };
    Ok((delivery, outcome == RuntimeWriteOutcome::Unchanged))
}

fn report(delivery: &Delivery<'_>, replayed: bool, facts: usize) -> IngestReport {
    IngestReport {
        namespace_id: delivery.namespace_id,
        ingestion_id: delivery.ingestion_id,
        replayed,
        facts,
        control: "none",
    }
}

pub(crate) async fn ingest_hook(
    persist: &PersistHandle,
    source: &ObserveSource,
    delivery_id: Uuid,
    raw: &[u8],
) -> Result<IngestReport, IngestError> {
    let payload = HookPayload::parse(raw).map_err(IngestError::BadInput)?;
    let namespace = namespace_for(source);
    let scope = payload.scope();
    let ids = EventIds {
        session: Some(payload.session_id.clone()),
        thread: Some(scope.thread_id),
        turn: payload.turn_id.clone(),
        item: payload.tool_use_id.clone(),
    };
    let extra = BTreeMap::from([("hook_event_name".to_owned(), json!(payload.hook_event_name))]);
    let envelope = envelope(&namespace, delivery_id, source, "codex_hook", ids, extra);
    let (delivery, replayed) = record(persist, &namespace, envelope, raw).await?;
    let facts = if replayed {
        0
    } else {
        apply::hook(&delivery, &payload).await?
    };
    Ok(report(&delivery, replayed, facts))
}

pub(crate) async fn ingest_otel(
    persist: &PersistHandle,
    source: &ObserveSource,
    signal: &str,
    raw: &[u8],
) -> Result<IngestReport, IngestError> {
    let body: Value = serde_json::from_slice(raw).map_err(|error| {
        IngestError::BadInput(format!(
            "OTLP {signal} body is not JSON at line {}, column {}: {error}; set protocol = \"json\" on the Codex otlp-http exporter",
            error.line(),
            error.column()
        ))
    })?;
    let mut source = source.clone();
    source.codex_version = source
        .codex_version
        .or_else(|| otel::resource_version(&body));
    let namespace = namespace_for(&source);
    let ingestion_id = otel_ingestion_id(&namespace, signal, raw);
    let ids = EventIds {
        session: None,
        thread: None,
        turn: None,
        item: None,
    };
    let extra = BTreeMap::from([("otlp_signal".to_owned(), json!(signal))]);
    let envelope = envelope(
        &namespace,
        ingestion_id,
        &source,
        &format!("codex_otel_{signal}"),
        ids,
        extra,
    );
    let (delivery, replayed) = record(persist, &namespace, envelope, raw).await?;
    if replayed {
        return Ok(report(&delivery, true, 0));
    }
    let facts = match signal {
        "logs" => apply::telemetry(&delivery, &otel::log_facts(&body)).await?,
        "traces" => {
            let runs = otel::hook_runs(&body);
            for run in &runs {
                writes::insert_hook_run(delivery.pool, delivery.namespace_id, run)
                    .await
                    .map_err(StoreError::from)?;
            }
            runs.len()
        }
        _ => 0,
    };
    Ok(report(&delivery, false, facts))
}
