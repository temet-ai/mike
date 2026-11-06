use serde::Deserialize;
use serde_json::value::RawValue;

use super::{NOTICES, Place, Removed, Seen, thin};

#[derive(Deserialize)]
struct Body<'a> {
    #[serde(borrow, default)]
    messages: Vec<&'a RawValue>,
    #[serde(borrow, default)]
    thread: Option<Thread<'a>>,
}

#[derive(Deserialize)]
struct Thread<'a> {
    #[serde(borrow, default, rename = "type")]
    kind: Option<&'a RawValue>,
}

#[derive(Deserialize)]
struct Message<'a> {
    #[serde(borrow, default)]
    role: Option<&'a RawValue>,
    #[serde(borrow, default)]
    content: Option<&'a RawValue>,
}

#[derive(Deserialize)]
struct Block<'a> {
    #[serde(borrow, default, rename = "type")]
    kind: Option<&'a RawValue>,
    #[serde(borrow, default)]
    text: Option<&'a RawValue>,
    #[serde(borrow, default)]
    content: Option<&'a RawValue>,
}

struct Edit {
    start: usize,
    end: usize,
    replacement: String,
}

struct Pass {
    seen: Seen,
    removed: Removed,
    edits: Vec<Edit>,
}

pub(crate) struct Scanned {
    pub(crate) thinned: Option<Vec<u8>>,
    pub(crate) removed: Removed,
    pub(crate) seen: Seen,
}

impl Body<'_> {
    fn continues_a_thread(&self) -> bool {
        self.thread
            .as_ref()
            .and_then(|thread| thread.kind)
            .is_some_and(|kind| kind.get() == "\"continue\"")
    }
}

pub(crate) fn continues_a_thread(raw: &[u8]) -> bool {
    serde_json::from_slice::<Body<'_>>(raw).is_ok_and(|body| body.continues_a_thread())
}

pub(crate) fn scan_body(raw: &[u8], seen_before: Seen) -> Option<Scanned> {
    let whole = std::str::from_utf8(raw).ok()?;
    if !may_hold_a_notice(whole) {
        return None;
    }
    let body: Body<'_> = serde_json::from_str(whole).ok()?;
    let mut pass = Pass {
        seen: if body.continues_a_thread() {
            seen_before
        } else {
            Seen::default()
        },
        removed: Removed::default(),
        edits: Vec::new(),
    };
    for message in &body.messages {
        scan_message(whole, message.get(), &mut pass);
    }
    Some(Scanned {
        thinned: (!pass.edits.is_empty()).then(|| spliced(whole, &pass.edits)),
        removed: pass.removed,
        seen: pass.seen,
    })
}

fn may_hold_a_notice(text: &str) -> bool {
    NOTICES.iter().any(|notice| notice.may_be_in(text))
}

fn scan_message(whole: &str, raw: &str, pass: &mut Pass) {
    if !may_hold_a_notice(raw) {
        return;
    }
    let Ok(message) = serde_json::from_str::<Message<'_>>(raw) else {
        return;
    };
    let Some(place) = message.role.and_then(|role| place_of(role.get())) else {
        return;
    };
    let Some(content) = message.content else {
        return;
    };
    for (literal, place) in text_literals(content.get(), place) {
        scan_literal(whole, literal, place, pass);
    }
}

fn place_of(role: &str) -> Option<Place> {
    match role {
        "\"user\"" => Some(Place::UserText),
        "\"system\"" => Some(Place::SystemText),
        _ => None,
    }
}

fn text_literals(content: &str, place: Place) -> Vec<(&str, Place)> {
    if content.starts_with('"') {
        return vec![(content, place)];
    }
    let Ok(blocks) = serde_json::from_str::<Vec<&RawValue>>(content) else {
        return Vec::new();
    };
    blocks
        .into_iter()
        .flat_map(|block| literals_of_block(block.get(), place))
        .collect()
}

fn literals_of_block(raw: &str, place: Place) -> Vec<(&str, Place)> {
    let Ok(block) = serde_json::from_str::<Block<'_>>(raw) else {
        return Vec::new();
    };
    match block.kind.map(RawValue::get) {
        Some("\"text\"") => block
            .text
            .map(RawValue::get)
            .filter(|literal| literal.starts_with('"'))
            .map(|literal| vec![(literal, place)])
            .unwrap_or_default(),
        Some("\"tool_result\"") if place == Place::UserText => block
            .content
            .map(|content| text_literals(content.get(), Place::ToolResult))
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn scan_literal(whole: &str, literal: &str, place: Place, pass: &mut Pass) {
    if !may_hold_a_notice(literal) {
        return;
    }
    let Some(start) = offset_in(whole, literal) else {
        return;
    };
    let Ok(text) = serde_json::from_str::<String>(literal) else {
        return;
    };
    let Some(thinned) = thin(&text, place, &mut pass.seen) else {
        return;
    };
    let Ok(replacement) = serde_json::to_string(&thinned.text) else {
        return;
    };
    for (total, removed) in pass.removed.iter_mut().zip(thinned.removed) {
        *total += removed;
    }
    pass.edits.push(Edit {
        start,
        end: start + literal.len(),
        replacement,
    });
}

fn offset_in(whole: &str, part: &str) -> Option<usize> {
    let start = (part.as_ptr() as usize).checked_sub(whole.as_ptr() as usize)?;
    let end = start.checked_add(part.len())?;
    (whole.get(start..end) == Some(part)).then_some(start)
}

fn spliced(whole: &str, edits: &[Edit]) -> Vec<u8> {
    let mut out = Vec::with_capacity(whole.len());
    let mut at = 0;
    for edit in edits {
        out.extend_from_slice(&whole.as_bytes()[at..edit.start]);
        out.extend_from_slice(edit.replacement.as_bytes());
        at = edit.end;
    }
    out.extend_from_slice(&whole.as_bytes()[at..]);
    out
}

#[cfg(test)]
#[path = "body_tests.rs"]
mod tests;
