use sqlx::PgConnection;

use crate::runtime::codex::{Evidence, ProjectionBatch, ProjectionEffect, ReceiptId};
use crate::runtime::validate_text;
use crate::store::persistence::PersistError;

use super::super::invalid;
use super::{CodexGeneration, StoredRuntimeReceipt, invariant};

struct EffectMetadata<'a> {
    kind: &'static str,
    thread: &'a str,
    turn: Option<&'a str>,
    item: Option<&'a str>,
    evidence: &'a Evidence,
}

fn metadata(effect: &ProjectionEffect) -> EffectMetadata<'_> {
    match effect {
        ProjectionEffect::Item(item) => EffectMetadata {
            kind: "item",
            thread: &item.key.thread_id,
            turn: Some(&item.key.turn_id),
            item: Some(&item.key.item_id),
            evidence: &item.evidence,
        },
        ProjectionEffect::Lifecycle(event) => EffectMetadata {
            kind: "lifecycle",
            thread: &event.thread_id,
            turn: event.turn_id.as_deref(),
            item: None,
            evidence: &event.evidence,
        },
        ProjectionEffect::Signal(signal) => EffectMetadata {
            kind: "signal",
            thread: &signal.thread_id,
            turn: signal.turn_id.value().map(String::as_str),
            item: signal.item_id.value().map(String::as_str),
            evidence: &signal.evidence,
        },
        ProjectionEffect::Usage(usage) => EffectMetadata {
            kind: "usage",
            thread: &usage.thread_id,
            turn: Some(&usage.turn_id),
            item: None,
            evidence: &usage.evidence,
        },
        ProjectionEffect::Billing(billing) => EffectMetadata {
            kind: "billing",
            thread: &billing.thread_id,
            turn: None,
            item: None,
            evidence: &billing.evidence,
        },
    }
}

pub fn validate_batch(
    batch: &ProjectionBatch,
    receipt: &StoredRuntimeReceipt,
) -> Result<(), PersistError> {
    let expected = ReceiptId {
        namespace_id: receipt.observation.envelope.namespace_id,
        ingestion_id: receipt.observation.envelope.ingestion_id,
    };
    if batch.receipt != expected {
        return Err(invariant("decoded batch refers to a different receipt"));
    }
    for effect in &batch.effects {
        let metadata = metadata(effect);
        if metadata.evidence.receipt != expected
            || (!metadata.evidence.json_pointer.is_empty()
                && !metadata.evidence.json_pointer.starts_with('/'))
        {
            return Err(invariant(
                "effect evidence does not belong to the decoded receipt",
            ));
        }
        for identifier in [Some(metadata.thread), metadata.turn, metadata.item]
            .into_iter()
            .flatten()
        {
            validate_text(identifier, "projection_identifier").map_err(invalid)?;
        }
        if let ProjectionEffect::Item(item) = effect
            && item.key.namespace_id != expected.namespace_id
        {
            return Err(invariant("item key belongs to a different namespace"));
        }
    }
    Ok(())
}

pub async fn insert_effects(
    connection: &mut PgConnection,
    generation: &CodexGeneration,
    receipt: &StoredRuntimeReceipt,
    batch: &ProjectionBatch,
) -> Result<(), PersistError> {
    let metadata: Vec<_> = batch.effects.iter().map(metadata).collect();
    let ordinals = (0..batch.effects.len())
        .map(i32::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| invariant("too many effects for one receipt"))?;
    let documents = batch
        .effects
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<_>, _>>()?;
    let kinds: Vec<String> = metadata.iter().map(|value| value.kind.to_owned()).collect();
    let threads: Vec<String> = metadata
        .iter()
        .map(|value| value.thread.to_owned())
        .collect();
    let turns: Vec<Option<String>> = metadata
        .iter()
        .map(|value| value.turn.map(str::to_owned))
        .collect();
    let items: Vec<Option<String>> = metadata
        .iter()
        .map(|value| value.item.map(str::to_owned))
        .collect();
    sqlx::query!(
        "INSERT INTO runtime.codex_projection_effects
         (generation_id, namespace_id, event_id, capture_sequence, effect_ordinal, effect_kind,
          thread_id, turn_id, item_id, observation)
         SELECT $1, $2, $3, $4, effect.ordinal, effect.kind, effect.thread_id, effect.turn_id,
                effect.item_id, effect.document::json
         FROM UNNEST($5::integer[], $6::text[], $7::text[], $8::text[], $9::text[], $10::text[])
         AS effect(ordinal, kind, thread_id, turn_id, item_id, document)",
        generation.generation_id,
        generation.namespace_id,
        receipt.event_id,
        receipt.capture_sequence,
        &ordinals as &[i32],
        &kinds as &[String],
        &threads as &[String],
        &turns as &[Option<String>],
        &items as &[Option<String>],
        &documents as &[String],
    )
    .execute(connection)
    .await?;
    Ok(())
}
