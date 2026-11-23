use serde_json::Value as JsonValue;

use crate::store::persistence::PersistError;
use crate::store::persistence::messages::{SentPatchRow, apply_sent_patches, invariant};
use crate::store::persistence::reconstruct::span::top_level_array_span;
use crate::store::persistence::reconstruct::{join_messages, splice_empty_arrays};

#[derive(Debug, Clone, Default)]
pub(crate) struct StoredRequest {
    pub id: i64,
    pub request_envelope: Option<Vec<u8>>,
    pub sent_envelope: Option<Vec<u8>>,
    pub sent_message_count: Option<i32>,
    pub envelope_patch: Option<SentPatchRow>,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RequestParts<'a> {
    pub original_messages: &'a [JsonValue],
    pub tools: &'a [u8],
    pub sent_messages: &'a [(i32, JsonValue)],
    pub delta_patches: &'a [SentPatchRow],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SentPath {
    Content,
    LegacyPatches,
}

impl StoredRequest {
    pub fn has_sent_content(&self, parts: RequestParts<'_>) -> bool {
        self.sent_envelope.is_some()
            || self.sent_message_count.is_some()
            || !parts.sent_messages.is_empty()
    }

    pub fn has_legacy_patches(&self, parts: RequestParts<'_>) -> bool {
        self.envelope_patch.is_some() || !parts.delta_patches.is_empty()
    }

    pub fn sent_path(&self, parts: RequestParts<'_>) -> SentPath {
        if !self.has_sent_content(parts) && self.has_legacy_patches(parts) {
            SentPath::LegacyPatches
        } else {
            SentPath::Content
        }
    }

    pub fn needs_raw_body(&self, parts: RequestParts<'_>) -> bool {
        match self.sent_path(parts) {
            SentPath::LegacyPatches => true,
            SentPath::Content => self.sent_envelope.is_none() && self.request_envelope.is_none(),
        }
    }

    pub fn legacy_patches(&self, parts: RequestParts<'_>) -> Vec<SentPatchRow> {
        parts
            .delta_patches
            .iter()
            .cloned()
            .chain(self.envelope_patch.clone())
            .collect()
    }
}

pub(crate) fn assemble_original(
    envelope: &[u8],
    parts: RequestParts<'_>,
) -> Result<Vec<u8>, PersistError> {
    let messages = join_messages(parts.original_messages)?;
    Ok(splice_empty_arrays(
        envelope,
        &[("messages", &messages), ("tools", parts.tools)],
    ))
}

pub(crate) fn assemble_sent(
    request: &StoredRequest,
    parts: RequestParts<'_>,
    raw_body: Option<&[u8]>,
) -> Result<Option<Vec<u8>>, PersistError> {
    match request.sent_path(parts) {
        SentPath::LegacyPatches => assemble_legacy(request, parts, raw_body),
        SentPath::Content => assemble_content(request, parts, raw_body),
    }
}

fn assemble_legacy(
    request: &StoredRequest,
    parts: RequestParts<'_>,
    raw_body: Option<&[u8]>,
) -> Result<Option<Vec<u8>>, PersistError> {
    let base = match (raw_body, request.request_envelope.as_deref()) {
        (Some(raw), _) => raw.to_vec(),
        (None, Some(envelope)) => assemble_original(envelope, parts)?,
        (None, None) => return Ok(None),
    };
    apply_sent_patches(&base, request.legacy_patches(parts)).map(Some)
}

fn assemble_content(
    request: &StoredRequest,
    parts: RequestParts<'_>,
    raw_body: Option<&[u8]>,
) -> Result<Option<Vec<u8>>, PersistError> {
    let Some(envelope) = request
        .sent_envelope
        .as_deref()
        .or(request.request_envelope.as_deref())
    else {
        return Ok(raw_body.map(<[u8]>::to_vec));
    };
    let messages = if top_level_array_span(envelope, "messages").is_some_and(|(s, e)| s == e) {
        join_messages(&sent_messages(request, parts)?)?
    } else {
        Vec::new()
    };
    Ok(Some(splice_empty_arrays(
        envelope,
        &[("messages", &messages), ("tools", parts.tools)],
    )))
}

fn sent_messages(
    request: &StoredRequest,
    parts: RequestParts<'_>,
) -> Result<Vec<JsonValue>, PersistError> {
    let Some(count) = request.sent_message_count else {
        return overridden_messages(request.id, parts);
    };
    let ords_complete = parts.sent_messages.len() == count as usize
        && parts
            .sent_messages
            .iter()
            .enumerate()
            .all(|(idx, (ord, _))| *ord as usize == idx);
    if !ords_complete {
        return Err(invariant(format!(
            "request {}: sent_message_count is {count} but requests.sent_messages holds ords {:?}",
            request.id,
            parts
                .sent_messages
                .iter()
                .map(|(ord, _)| *ord)
                .collect::<Vec<_>>()
        )));
    }
    Ok(parts
        .sent_messages
        .iter()
        .map(|(_, message)| message.clone())
        .collect())
}

fn overridden_messages(
    request_id: i64,
    parts: RequestParts<'_>,
) -> Result<Vec<JsonValue>, PersistError> {
    let mut messages = parts.original_messages.to_vec();
    for (ord, message) in parts.sent_messages {
        let Some(slot) = usize::try_from(*ord).ok().and_then(|o| messages.get_mut(o)) else {
            return Err(invariant(format!(
                "request {request_id}: requests.sent_messages ord {ord} is past the {} original messages",
                messages.len()
            )));
        };
        *slot = message.clone();
    }
    Ok(messages)
}

pub(crate) fn same_json(left: &[u8], right: &[u8]) -> bool {
    match (
        serde_json::from_slice::<JsonValue>(left),
        serde_json::from_slice::<JsonValue>(right),
    ) {
        (Ok(l), Ok(r)) => l == r,
        _ => false,
    }
}

#[cfg(test)]
#[path = "assemble_tests.rs"]
mod assemble_tests;
