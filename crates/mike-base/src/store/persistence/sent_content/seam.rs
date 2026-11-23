use serde_json::Value as JsonValue;

use super::assemble::{RequestParts, StoredRequest, assemble_sent};
use super::derive::{
    MessageKey, SentMessagesPlan, plan_sent_messages, sent_envelope, sent_messages_split,
};
use crate::store::persistence::PersistError;
use crate::store::persistence::messages::{CanonicalMessage, canonical_clean_messages, invariant};
use crate::store::persistence::reconstruct::span::top_level_array_span;
use crate::store::persistence::reconstruct::{restore_cache_control, strip_bulk_arrays};

fn stored_form(message: &CanonicalMessage) -> Result<JsonValue, PersistError> {
    let mut value: JsonValue = serde_json::from_slice(&message.body_canonical_bytes)?;
    restore_cache_control(&mut value, message.cache_control.as_ref());
    Ok(value)
}

pub fn sent_round_trip_in_memory(original: &[u8], sent: &[u8]) -> Result<Vec<u8>, PersistError> {
    let canonical = canonical_clean_messages(original)?;
    let original_messages = canonical
        .iter()
        .map(stored_form)
        .collect::<Result<Vec<_>, _>>()?;
    let tools = top_level_array_span(original, "tools")
        .map(|(start, end)| original[start..end].to_vec())
        .unwrap_or_default();
    let request_envelope = strip_bulk_arrays(original);
    let plan = if original != sent && sent_messages_split(sent) {
        let keys: Vec<MessageKey<'_>> = canonical.iter().map(MessageKey::from).collect();
        plan_sent_messages(&keys, canonical_clean_messages(sent)?)
    } else {
        SentMessagesPlan::default()
    };
    let sent_messages = plan
        .rows
        .iter()
        .map(|m| Ok((m.ord, stored_form(m)?)))
        .collect::<Result<Vec<_>, PersistError>>()?;
    let request = StoredRequest {
        id: 0,
        sent_envelope: sent_envelope(original, request_envelope.as_deref(), sent),
        request_envelope,
        sent_message_count: plan.count,
        envelope_patch: None,
    };
    let parts = RequestParts {
        original_messages: &original_messages,
        tools: &tools,
        sent_messages: &sent_messages,
        delta_patches: &[],
    };
    assemble_sent(&request, parts, None)?.ok_or_else(|| invariant("the sent body did not rebuild"))
}
