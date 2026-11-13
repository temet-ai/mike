use std::collections::HashMap;

use sqlx::PgConnection;
use uuid::Uuid;

use crate::runtime::codex::{
    ItemKey, ProjectionBatch, ProjectionEffect, ReceiptApplication, reduce_item,
};
use crate::store::persistence::PersistError;

use super::codec::{STATE_CODEC_VERSION, decode_item, encode_item};
use super::types::CodexStoredItem;
use super::{CodexGeneration, invariant};

pub struct ItemRow {
    pub namespace_id: Uuid,
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub first_capture_sequence: i64,
    pub latest_capture_sequence: i64,
    pub state_codec_version: i32,
    pub state_document: String,
}

impl TryFrom<ItemRow> for CodexStoredItem {
    type Error = PersistError;

    fn try_from(row: ItemRow) -> Result<Self, Self::Error> {
        if row.state_codec_version != STATE_CODEC_VERSION {
            return Err(invariant("unsupported stored item codec version"));
        }
        let key = ItemKey {
            namespace_id: row.namespace_id,
            thread_id: row.thread_id,
            turn_id: row.turn_id,
            item_id: row.item_id,
        };
        Ok(Self {
            first_capture_sequence: row.first_capture_sequence,
            latest_capture_sequence: row.latest_capture_sequence,
            item: decode_item(&row.state_document, &key)?,
        })
    }
}

pub async fn reduce_states(
    connection: &mut PgConnection,
    generation: &CodexGeneration,
    sequence: i64,
    batch: &ProjectionBatch,
) -> Result<(), PersistError> {
    let keys: Vec<_> = batch
        .effects
        .iter()
        .filter_map(|effect| match effect {
            ProjectionEffect::Item(item) => Some(item.key.clone()),
            _ => None,
        })
        .collect();
    if keys.is_empty() {
        return Ok(());
    }
    let mut states = load_states(connection, generation.generation_id, &keys).await?;
    for effect in &batch.effects {
        let ProjectionEffect::Item(observation) = effect else {
            continue;
        };
        let current = states.get(&observation.key);
        let reduction = reduce_item(
            current.map(|state| &state.item),
            observation,
            ReceiptApplication::Unapplied,
        );
        let item = reduction
            .item
            .ok_or_else(|| invariant("item reducer produced no state"))?;
        let first = current.map_or(sequence, |state| state.first_capture_sequence);
        states.insert(
            item.key.clone(),
            CodexStoredItem {
                first_capture_sequence: first,
                latest_capture_sequence: sequence,
                item,
            },
        );
    }
    save_states(connection, generation, states.into_values().collect()).await
}

async fn load_states(
    connection: &mut PgConnection,
    generation_id: Uuid,
    keys: &[ItemKey],
) -> Result<HashMap<ItemKey, CodexStoredItem>, PersistError> {
    let threads: Vec<String> = keys.iter().map(|key| key.thread_id.clone()).collect();
    let turns: Vec<String> = keys.iter().map(|key| key.turn_id.clone()).collect();
    let items: Vec<String> = keys.iter().map(|key| key.item_id.clone()).collect();
    let rows = sqlx::query_file_as!(
        ItemRow,
        "src/store/persistence/runtime/codex/load_item_states.sql",
        generation_id,
        &threads as &[String],
        &turns as &[String],
        &items as &[String],
    )
    .fetch_all(connection)
    .await?;
    let states = rows
        .into_iter()
        .map(CodexStoredItem::try_from)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(states
        .into_iter()
        .map(|state| (state.item.key.clone(), state))
        .collect())
}

async fn save_states(
    connection: &mut PgConnection,
    generation: &CodexGeneration,
    states: Vec<CodexStoredItem>,
) -> Result<(), PersistError> {
    let documents = states
        .iter()
        .map(|state| encode_item(&state.item))
        .collect::<Result<Vec<_>, _>>()?;
    let threads: Vec<String> = states
        .iter()
        .map(|state| state.item.key.thread_id.clone())
        .collect();
    let turns: Vec<String> = states
        .iter()
        .map(|state| state.item.key.turn_id.clone())
        .collect();
    let items: Vec<String> = states
        .iter()
        .map(|state| state.item.key.item_id.clone())
        .collect();
    let terminal: Vec<bool> = states.iter().map(|state| state.item.terminal).collect();
    let reconciliation: Vec<bool> = states
        .iter()
        .map(|state| state.item.reconciliation_required)
        .collect();
    let first: Vec<i64> = states
        .iter()
        .map(|state| state.first_capture_sequence)
        .collect();
    let latest: Vec<i64> = states
        .iter()
        .map(|state| state.latest_capture_sequence)
        .collect();
    sqlx::query!(
        "INSERT INTO runtime.codex_item_state
         (generation_id, namespace_id, thread_id, turn_id, item_id, terminal, reconciliation_required,
          first_capture_sequence, latest_capture_sequence, state_codec_version, state_document)
         SELECT $1, $2, item.thread_id, item.turn_id, item.item_id, item.terminal, item.reconciliation,
                item.first_sequence, item.latest_sequence, $3, item.document::json
         FROM UNNEST($4::text[], $5::text[], $6::text[], $7::boolean[], $8::boolean[],
                     $9::bigint[], $10::bigint[], $11::text[])
         AS item(thread_id, turn_id, item_id, terminal, reconciliation, first_sequence, latest_sequence, document)
         ON CONFLICT (generation_id, thread_id, turn_id, item_id) DO UPDATE
         SET terminal = EXCLUDED.terminal, reconciliation_required = EXCLUDED.reconciliation_required,
             latest_capture_sequence = EXCLUDED.latest_capture_sequence, state_document = EXCLUDED.state_document",
        generation.generation_id,
        generation.namespace_id,
        STATE_CODEC_VERSION,
        &threads as &[String],
        &turns as &[String],
        &items as &[String],
        &terminal as &[bool],
        &reconciliation as &[bool],
        &first as &[i64],
        &latest as &[i64],
        &documents as &[String],
    )
    .execute(connection)
    .await?;
    Ok(())
}
