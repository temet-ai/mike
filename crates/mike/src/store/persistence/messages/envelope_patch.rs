use std::collections::HashMap;

use crate::compress::manifest::ByteRange;
use crate::store::persistence::PersistError;

use super::canonicalize::invariant;
use super::delta::SentPatch;
use super::sent_patch::locate_message_spans;

pub(crate) fn compute_envelope_patch(
    request_body: &[u8],
    sent_body: &[u8],
) -> Result<Option<SentPatch>, PersistError> {
    let request_spans = locate_message_spans(request_body)?;
    let sent_spans = locate_message_spans(sent_body)?;
    if request_spans.len() != sent_spans.len() {
        return whole_body_envelope_patch(request_body, sent_body);
    }

    let (Some(req_fields), Some(sent_fields)) = (
        locate_top_level_fields(request_body),
        locate_top_level_fields(sent_body),
    ) else {
        return whole_body_envelope_patch(request_body, sent_body);
    };

    let mut all_keys: Vec<&str> = req_fields
        .keys()
        .chain(sent_fields.keys())
        .map(String::as_str)
        .collect();
    all_keys.sort_unstable();
    all_keys.dedup();

    let changed_keys: Vec<&str> = all_keys
        .into_iter()
        .filter(|key| {
            *key != "messages"
                && match (req_fields.get(*key), sent_fields.get(*key)) {
                    (Some(r), Some(s)) => request_body[r.start..r.end] != sent_body[s.start..s.end],
                    (None, None) => false,
                    _ => true,
                }
        })
        .collect();

    let [key] = changed_keys.as_slice() else {
        if changed_keys.len() > 1 {
            return multi_key_envelope_patch(
                request_body,
                sent_body,
                &req_fields,
                &sent_fields,
                &changed_keys,
            );
        }
        return Ok(None);
    };
    let (Some(req_range), Some(sent_range)) = (req_fields.get(*key), sent_fields.get(*key)) else {
        return Ok(None);
    };

    build_patch(
        request_body,
        req_range.start,
        req_range.end,
        sent_body,
        sent_range.start,
        sent_range.end,
    )
}

fn multi_key_envelope_patch(
    request_body: &[u8],
    sent_body: &[u8],
    req_fields: &HashMap<String, ByteRange>,
    sent_fields: &HashMap<String, ByteRange>,
    changed_keys: &[&str],
) -> Result<Option<SentPatch>, PersistError> {
    let bail = |reason: &str| {
        tracing::debug!(
            changed_keys = ?changed_keys,
            reason,
            "compute_envelope_patch: the changed top-level fields have no covering span"
        );
        Ok(None)
    };

    let mut req_lo = usize::MAX;
    let mut req_hi = 0usize;
    let mut sent_lo = usize::MAX;
    let mut sent_hi = 0usize;
    for key in changed_keys {
        let (Some(r), Some(s)) = (req_fields.get(*key), sent_fields.get(*key)) else {
            return bail("a changed key is absent from one of the two bodies");
        };
        req_lo = req_lo.min(r.start);
        req_hi = req_hi.max(r.end);
        sent_lo = sent_lo.min(s.start);
        sent_hi = sent_hi.max(s.end);
    }

    let (req_messages, sent_messages) = (req_fields.get("messages"), sent_fields.get("messages"));
    if req_messages.is_some() != sent_messages.is_some() {
        return bail("messages[] is present in only one of the two bodies");
    }

    if let Some(m) = req_messages
        && m.start < req_hi
        && req_lo < m.end
    {
        return bail("the covering span contains messages[]");
    }

    let req_regions = regions_outside(request_body.len(), (req_lo, req_hi), req_messages);
    let sent_regions = regions_outside(sent_body.len(), (sent_lo, sent_hi), sent_messages);
    if req_regions.len() != sent_regions.len() {
        return bail("the two bodies do not decompose into matching regions");
    }
    for (r, s) in req_regions.iter().zip(&sent_regions) {
        let (Some(rb), Some(sb)) = (request_body.get(r.0..r.1), sent_body.get(s.0..s.1)) else {
            return bail("a region ran outside its body");
        };
        if rb != sb {
            return bail("bytes outside the covering span and messages[] differ");
        }
    }

    build_patch(request_body, req_lo, req_hi, sent_body, sent_lo, sent_hi)
}

fn regions_outside(
    len: usize,
    span: (usize, usize),
    messages: Option<&ByteRange>,
) -> Vec<(usize, usize)> {
    let mut holes = vec![span];
    if let Some(m) = messages {
        holes.push((m.start, m.end));
    }
    holes.sort_unstable();

    let mut out = Vec::with_capacity(holes.len() + 1);
    let mut cursor = 0usize;
    for (start, end) in holes {
        if start > cursor {
            out.push((cursor, start));
        }
        cursor = cursor.max(end);
    }
    if cursor < len {
        out.push((cursor, len));
    }
    out
}

pub(crate) fn whole_body_envelope_patch(
    request_body: &[u8],
    sent_body: &[u8],
) -> Result<Option<SentPatch>, PersistError> {
    build_patch(
        request_body,
        0,
        request_body.len(),
        sent_body,
        0,
        sent_body.len(),
    )
}

fn build_patch(
    request_body: &[u8],
    req_start: usize,
    req_end: usize,
    sent_body: &[u8],
    sent_start: usize,
    sent_end: usize,
) -> Result<Option<SentPatch>, PersistError> {
    let req_bytes = &request_body[req_start..req_end];
    let sent_bytes = &sent_body[sent_start..sent_end];
    if req_bytes == sent_bytes {
        return Ok(None);
    }
    let (prefix, suffix) = common_prefix_suffix(req_bytes, sent_bytes);
    let new_start = req_start + prefix;
    let old_end = req_start + (req_bytes.len() - suffix);
    let start = i32::try_from(new_start).map_err(|_| {
        invariant(format!(
            "envelope patch start {new_start} exceeds INT32_MAX"
        ))
    })?;
    let end = i32::try_from(old_end)
        .map_err(|_| invariant(format!("envelope patch end {old_end} exceeds INT32_MAX")))?;
    Ok(Some(SentPatch {
        start,
        end,
        bytes: sent_bytes[prefix..sent_bytes.len() - suffix].to_vec(),
    }))
}

pub(super) fn locate_top_level_fields(raw_body: &[u8]) -> Option<HashMap<String, ByteRange>> {
    let parsed: HashMap<&str, &serde_json::value::RawValue> =
        serde_json::from_slice(raw_body).ok()?;
    let base = raw_body.as_ptr() as usize;
    Some(
        parsed
            .into_iter()
            .map(|(key, rv)| {
                let text = rv.get();
                let start = text.as_ptr() as usize - base;
                (
                    key.to_string(),
                    ByteRange {
                        start,
                        end: start + text.len(),
                    },
                )
            })
            .collect(),
    )
}

pub(super) fn common_prefix_suffix(a: &[u8], b: &[u8]) -> (usize, usize) {
    let max_prefix = a.len().min(b.len());
    let mut prefix = 0;
    while prefix < max_prefix && a[prefix] == b[prefix] {
        prefix += 1;
    }
    let max_suffix = a.len().min(b.len()) - prefix;
    let mut suffix = 0;
    while suffix < max_suffix && a[a.len() - 1 - suffix] == b[b.len() - 1 - suffix] {
        suffix += 1;
    }
    (prefix, suffix)
}
