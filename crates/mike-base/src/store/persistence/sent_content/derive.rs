use serde_json::Value as JsonValue;

use crate::store::persistence::messages::CanonicalMessage;
use crate::store::persistence::reconstruct::span::top_level_array_span;

#[derive(Debug, Clone, Copy)]
pub(crate) struct MessageKey<'a> {
    pub sha: &'a str,
    pub cache_control: Option<&'a JsonValue>,
}

impl<'a> From<&'a CanonicalMessage> for MessageKey<'a> {
    fn from(message: &'a CanonicalMessage) -> Self {
        Self {
            sha: &message.sha,
            cache_control: message.cache_control.as_ref(),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct SentMessagesPlan {
    pub count: Option<i32>,
    pub rows: Vec<CanonicalMessage>,
}

pub(crate) fn sent_messages_split(sent: &[u8]) -> bool {
    top_level_array_span(sent, "messages").is_some()
}

pub(crate) fn sent_envelope(
    original: &[u8],
    original_envelope: Option<&[u8]>,
    sent: &[u8],
) -> Option<Vec<u8>> {
    if original == sent {
        return None;
    }
    let candidate = sent_envelope_candidate(original, sent);
    (original_envelope != Some(candidate.as_slice())).then_some(candidate)
}

pub(crate) fn sent_envelope_candidate(original: &[u8], sent: &[u8]) -> Vec<u8> {
    let mut edits: Vec<Edit> = Vec::with_capacity(2);
    if let Some((start, end)) = top_level_array_span(sent, "messages") {
        edits.push(Edit::Drain(start, end));
    }
    edits.extend(tools_edit(original, sent));
    apply_edits(sent, edits)
}

pub(crate) fn whole_sent_envelope(original: &[u8], sent: &[u8]) -> Vec<u8> {
    apply_edits(sent, tools_edit(original, sent).into_iter().collect())
}

enum Edit {
    Drain(usize, usize),
    Space(usize),
}

impl Edit {
    fn at(&self) -> usize {
        match self {
            Self::Drain(start, _) | Self::Space(start) => *start,
        }
    }
}

fn tools_edit(original: &[u8], sent: &[u8]) -> Option<Edit> {
    let (start, end) = top_level_array_span(sent, "tools")?;
    let original_tools = top_level_array_span(original, "tools")
        .map(|(o_start, o_end)| &original[o_start..o_end])
        .unwrap_or_default();
    let sent_tools = &sent[start..end];
    if sent_tools.is_empty() {
        return (!original_tools.is_empty()).then_some(Edit::Space(start));
    }
    (sent_tools == original_tools).then_some(Edit::Drain(start, end))
}

fn apply_edits(body: &[u8], mut edits: Vec<Edit>) -> Vec<u8> {
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.at()));
    let mut out = body.to_vec();
    for edit in edits {
        match edit {
            Edit::Drain(start, end) => {
                out.drain(start..end);
            }
            Edit::Space(at) => out.insert(at, b' '),
        }
    }
    out
}

pub(crate) fn plan_sent_messages(
    original: &[MessageKey<'_>],
    sent: Vec<CanonicalMessage>,
) -> SentMessagesPlan {
    if original.len() != sent.len() {
        return SentMessagesPlan {
            count: i32::try_from(sent.len()).ok(),
            rows: sent,
        };
    }
    let rows = sent
        .into_iter()
        .zip(original)
        .filter(|(message, key)| {
            message.sha != key.sha || message.cache_control.as_ref() != key.cache_control
        })
        .map(|(message, _)| message)
        .collect();
    SentMessagesPlan { count: None, rows }
}

#[cfg(test)]
#[path = "derive_tests.rs"]
mod derive_tests;
