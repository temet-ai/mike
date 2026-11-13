use mike::store::persistence::runtime_repo::RuntimePersist as _;
use std::collections::BTreeMap;

use chrono::Utc;
use mike::runtime::{RuntimeEventEnvelope, RuntimeEventSource, RuntimeKind, RuntimeNamespace};
use mike::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};
use serde_json::{Value, json};
use uuid::Uuid;

use mike::runtime::codex::{IntakeContext, ItemKey, ReceiptId};
use mike::store::persistence::PersistHandle;

use super::{
    ClassificationProvenance, CodexCaptureOutcome, CodexEffectPage, CodexIntakeContext,
    CodexReceiptInput, StoredRuntimeReceipt,
};

pub(super) fn input(namespace_id: Uuid, payload: Value) -> anyhow::Result<CodexReceiptInput> {
    input_at(
        namespace_id,
        serde_json::to_vec(&payload)?,
        Uuid::new_v4(),
        0,
        IntakeContext::ordinary(),
    )
}

pub(super) fn input_at(
    namespace_id: Uuid,
    raw_payload: Vec<u8>,
    epoch: Uuid,
    ordinal: i64,
    context: IntakeContext<'_>,
) -> anyhow::Result<CodexReceiptInput> {
    let envelope = envelope(namespace_id);
    Ok(CodexReceiptInput {
        envelope,
        raw_payload,
        context: CodexIntakeContext::new(
            context,
            epoch,
            ordinal,
            ClassificationProvenance::ReviewedOrdinaryRoute,
        )?,
    })
}

pub(super) fn captured(outcome: CodexCaptureOutcome) -> anyhow::Result<StoredRuntimeReceipt> {
    match outcome {
        CodexCaptureOutcome::Captured { receipt, .. } => Ok(*receipt),
        CodexCaptureOutcome::Excluded(reason) => anyhow::bail!("unexpected exclusion: {reason:?}"),
    }
}

pub(super) fn receipt_id(input: &CodexReceiptInput) -> ReceiptId {
    ReceiptId {
        namespace_id: input.envelope.namespace_id,
        ingestion_id: input.envelope.ingestion_id,
    }
}

pub(super) fn key(namespace_id: Uuid) -> ItemKey {
    ItemKey {
        namespace_id,
        thread_id: "thread".into(),
        turn_id: "turn".into(),
        item_id: "item".into(),
    }
}

pub(super) fn delta(text: &str) -> Value {
    json!({"method":"item/agentMessage/delta", "params":{
        "threadId":"thread", "turnId":"turn", "itemId":"item", "delta":text
    }})
}

pub(super) fn page(generation_id: Uuid) -> CodexEffectPage {
    CodexEffectPage {
        generation_id,
        thread_id: "thread".into(),
        kind: None,
        after: None,
        limit: 100,
    }
}

pub(super) async fn count(
    handle: &PersistHandle,
    namespace: Uuid,
    table: &str,
) -> anyhow::Result<i64> {
    let query = match table {
        "events" => "SELECT count(*) FROM runtime.events WHERE namespace_id = $1",
        "context" => "SELECT count(*) FROM runtime.codex_receipt_context WHERE namespace_id = $1",
        "effects" => {
            "SELECT count(*) FROM runtime.codex_projection_effects WHERE namespace_id = $1"
        }
        "items" => "SELECT count(*) FROM runtime.codex_item_state WHERE namespace_id = $1",
        "applications" => {
            "SELECT count(*) FROM runtime.codex_projection_applications WHERE namespace_id = $1"
        }
        _ => anyhow::bail!("unknown fixture table"),
    };
    Ok(sqlx::query_scalar(query)
        .bind(namespace)
        .fetch_one(handle.pool())
        .await?)
}

pub(super) async fn database()
-> anyhow::Result<Option<(TestDbHandle, PersistHandle, RuntimeNamespace)>> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(None);
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let namespace = namespace();
    handle
        .runtime()
        .register_runtime_namespace(&namespace)
        .await?;
    Ok(Some((db, handle, namespace)))
}

pub(super) fn namespace() -> RuntimeNamespace {
    RuntimeNamespace {
        id: Uuid::new_v4(),
        runtime: RuntimeKind::Codex,
        durable_key: format!("fixture/{}", Uuid::new_v4()),
    }
}

fn envelope(namespace_id: Uuid) -> RuntimeEventEnvelope {
    RuntimeEventEnvelope {
        version: 1,
        ingestion_id: Uuid::new_v4(),
        namespace_id,
        source: RuntimeEventSource {
            kind: "fixture".into(),
            source_version: None,
            adapter: "offline".into(),
            adapter_version: "1".into(),
            runtime_version: None,
            fidelity: "original_event_bytes".into(),
            completeness: "unknown".into(),
            extra: BTreeMap::new(),
        },
        received_at: Utc::now(),
        provider_timestamp: None,
        provider_session_id: None,
        provider_thread_id: None,
        provider_turn_id: None,
        provider_item_id: None,
        provider_event_id: None,
        source_cursor: Some("cursor:1".into()),
        extra: BTreeMap::new(),
    }
}
