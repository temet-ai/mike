use std::collections::HashMap;

use crate::compress::manifest::ByteRange;
use crate::store::persistence::PersistError;

use super::canonicalize::{CanonicalMessage, invariant};
use super::delta::SentPatch;

pub(super) fn locate_message_spans(raw_body: &[u8]) -> Result<Vec<ByteRange>, PersistError> {
    #[derive(serde::Deserialize)]
    struct Shape<'a> {
        #[serde(borrow, default)]
        messages: Vec<&'a serde_json::value::RawValue>,
    }
    let parsed: Shape = serde_json::from_slice(raw_body)?;
    let base = raw_body.as_ptr() as usize;
    Ok(parsed
        .messages
        .iter()
        .map(|rv| {
            let text = rv.get();
            let start = text.as_ptr() as usize - base;
            ByteRange {
                start,
                end: start + text.len(),
            }
        })
        .collect())
}

pub(crate) fn compute_sent_patches(
    request_body: &[u8],
    sent_body: &[u8],
    canonical: &[CanonicalMessage],
) -> Result<HashMap<i32, SentPatch>, PersistError> {
    let request_spans = locate_message_spans(request_body)?;
    let sent_spans = locate_message_spans(sent_body)?;
    if request_spans.len() != sent_spans.len() || request_spans.len() != canonical.len() {
        return Ok(HashMap::new());
    }
    let mut out = HashMap::with_capacity(canonical.len());
    for (m, (req_span, sent_span)) in canonical
        .iter()
        .zip(request_spans.iter().zip(sent_spans.iter()))
    {
        let req_bytes = &request_body[req_span.start..req_span.end];
        let sent_bytes = &sent_body[sent_span.start..sent_span.end];
        if req_bytes != sent_bytes {
            let start = i32::try_from(req_span.start).map_err(|_| {
                invariant(format!(
                    "messages[{}] request_body span start {} exceeds INT32_MAX",
                    m.ord, req_span.start
                ))
            })?;
            let end = i32::try_from(req_span.end).map_err(|_| {
                invariant(format!(
                    "messages[{}] request_body span end {} exceeds INT32_MAX",
                    m.ord, req_span.end
                ))
            })?;
            out.insert(
                m.ord,
                SentPatch {
                    start,
                    end,
                    bytes: sent_bytes.to_vec(),
                },
            );
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct SentPatchRow {
    pub(crate) sent_patch_start: i32,
    pub(crate) sent_patch_end: i32,
    pub(crate) sent_patch_bytes: Vec<u8>,
}

pub(crate) fn apply_sent_patches(
    original: &[u8],
    mut patches: Vec<SentPatchRow>,
) -> Result<Vec<u8>, PersistError> {
    patches.sort_by_key(|p| p.sent_patch_start);
    let mut out = Vec::with_capacity(original.len());
    let mut cursor: usize = 0;
    for p in &patches {
        let start = usize::try_from(p.sent_patch_start)
            .map_err(|_| invariant("sent_patch_start is negative"))?;
        let end = usize::try_from(p.sent_patch_end)
            .map_err(|_| invariant("sent_patch_end is negative"))?;
        if start < cursor || end > original.len() || start > end {
            return Err(invariant(format!(
                "sent patch [{start}, {end}) is out of range or overlaps a prior patch \
                 (cursor={cursor}, original.len()={})",
                original.len()
            )));
        }
        out.extend_from_slice(&original[cursor..start]);
        out.extend_from_slice(&p.sent_patch_bytes);
        cursor = end;
    }
    out.extend_from_slice(&original[cursor..]);
    Ok(out)
}
