use std::borrow::Cow;
use std::collections::HashMap;

use sha2::{Digest, Sha256};

use serde::Deserialize;
use serde_json::Value;
use serde_json::value::RawValue;

const PLAIN_STRING_OPEN: &[u8] = br#"[{"type":"text","text":"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InjectedBlocks {
    pub identity: u64,
    tail: Vec<u8>,
}

impl InjectedBlocks {
    pub fn restored(identity: u64, tail: Vec<u8>) -> Option<Self> {
        (tail.first() == Some(&b',') && tail.last() == Some(&b']'))
            .then_some(Self { identity, tail })
    }

    pub fn tail(&self) -> &[u8] {
        &self.tail
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Spliced {
    pub body: Vec<u8>,
    pub message_indices: Vec<usize>,
    pub continuation: bool,
}

#[derive(Deserialize)]
struct MessagesShape<'a> {
    #[serde(borrow, default)]
    messages: Vec<&'a RawValue>,
    #[serde(borrow, default)]
    thread: Option<&'a RawValue>,
}

#[derive(Deserialize)]
struct ThreadShape<'a> {
    #[serde(borrow, default, rename = "type")]
    kind: Option<Cow<'a, str>>,
}

impl MessagesShape<'_> {
    fn is_continuation(&self) -> bool {
        self.thread
            .and_then(|t| serde_json::from_str::<ThreadShape<'_>>(t.get()).ok())
            .and_then(|t| t.kind)
            .is_some_and(|k| k == "continue")
    }
}

#[derive(Deserialize)]
struct MessageShape<'a> {
    #[serde(borrow, default)]
    role: Option<Cow<'a, str>>,
    #[serde(borrow)]
    content: &'a RawValue,
}

#[derive(Deserialize)]
struct ContentLen<'a> {
    #[serde(borrow, default)]
    items: Vec<&'a RawValue>,
}

fn shape(body: &[u8]) -> Option<MessagesShape<'_>> {
    serde_json::from_slice::<MessagesShape<'_>>(body).ok()
}

fn messages(body: &[u8]) -> Option<Vec<&RawValue>> {
    shape(body).map(|m| m.messages)
}

fn span_in(body: &[u8], inner: &str) -> Option<(usize, usize)> {
    let start = (inner.as_ptr() as usize).checked_sub(body.as_ptr() as usize)?;
    let end = start.checked_add(inner.len())?;
    (end <= body.len()).then_some((start, end))
}

fn content_span(body: &[u8], message: &RawValue) -> Option<(usize, usize)> {
    let shape: MessageShape<'_> = serde_json::from_str(message.get()).ok()?;
    span_in(body, shape.content.get())
}

fn is_user(message: &RawValue) -> bool {
    serde_json::from_str::<MessageShape<'_>>(message.get())
        .ok()
        .and_then(|m| m.role)
        .is_some_and(|r| r == "user")
}

fn array_len(content: &[u8]) -> Option<usize> {
    let wrapped = [br#"{"items":"#.as_slice(), content, b"}"].concat();
    serde_json::from_slice::<ContentLen<'_>>(&wrapped)
        .ok()
        .map(|c| c.items.len())
}

fn strip_cache_control(value: &mut Value) {
    if let Some(obj) = value.as_object_mut() {
        obj.remove("cache_control");
        if let Some(Value::Array(items)) = obj.get_mut("content") {
            items.iter_mut().for_each(strip_cache_control);
        }
    }
}

fn content_as_blocks(value: &mut Value) {
    if let Some(Value::String(text)) = value.get("content") {
        let blocks = serde_json::json!([{ "type": "text", "text": text }]);
        value["content"] = blocks;
    }
}

fn identity(message: &RawValue) -> Option<u64> {
    let mut value: Value = serde_json::from_str(message.get()).ok()?;
    content_as_blocks(&mut value);
    strip_cache_control(&mut value);
    let canonical = serde_json::to_vec(&value).ok()?;
    let digest = Sha256::digest(&canonical);
    let mut first = [0u8; 8];
    first.copy_from_slice(&digest[..8]);
    Some(u64::from_be_bytes(first))
}

fn plain_string_prefix(original: &[u8]) -> Vec<u8> {
    [PLAIN_STRING_OPEN, original, b"}"].concat()
}

fn appended_tail(original: &[u8], injected: &[u8]) -> Option<Vec<u8>> {
    let tail = match original.first()? {
        b'[' => {
            let open = original.len().checked_sub(1)?;
            let grew = array_len(injected)? > array_len(original)?;
            (original.last() == Some(&b']')
                && grew
                && injected.len() > original.len()
                && injected.get(..open)? == original.get(..open)?)
            .then(|| injected[open..].to_vec())?
        }
        b'"' => injected
            .strip_prefix(plain_string_prefix(original).as_slice())?
            .to_vec(),
        _ => return None,
    };
    (tail.first() == Some(&b',') && tail.last() == Some(&b']')).then_some(tail)
}

pub fn capture(original: &[u8], injected: &[u8]) -> Option<InjectedBlocks> {
    let before = messages(original)?;
    let after = messages(injected)?;
    if before.len() != after.len() {
        return None;
    }
    let index = after.iter().rposition(|m| is_user(m))?;
    let (os, oe) = content_span(original, before[index])?;
    let (is, ie) = content_span(injected, after[index])?;
    let tail = appended_tail(&original[os..oe], &injected[is..ie])?;
    Some(InjectedBlocks {
        identity: identity(before[index])?,
        tail,
    })
}

fn with_tail(content: &[u8], tail: &[u8]) -> Option<Vec<u8>> {
    match content.first()? {
        b'[' if content.len() > 2 && content.last() == Some(&b']') => {
            Some([&content[..content.len() - 1], tail].concat())
        }
        b'"' => Some([plain_string_prefix(content).as_slice(), tail].concat()),
        _ => None,
    }
}

struct Edit<'r> {
    index: usize,
    start: usize,
    end: usize,
    tail: &'r [u8],
}

fn plan_edits<'r>(
    body: &[u8],
    messages: Vec<&RawValue>,
    records: &'r HashMap<u64, InjectedBlocks>,
) -> Vec<Edit<'r>> {
    let mut last_match: HashMap<u64, Edit<'r>> = HashMap::new();
    for (index, message) in messages.into_iter().enumerate() {
        if !is_user(message) {
            continue;
        }
        let Some(record) = identity(message).and_then(|id| records.get(&id)) else {
            continue;
        };
        let Some((start, end)) = content_span(body, message) else {
            continue;
        };
        let edit = Edit {
            index,
            start,
            end,
            tail: &record.tail,
        };
        last_match.insert(record.identity, edit);
    }
    let mut edits: Vec<Edit<'r>> = last_match.into_values().collect();
    edits.sort_by_key(|e| e.start);
    edits
}

pub fn splice_all(body: &[u8], records: &HashMap<u64, InjectedBlocks>) -> Option<Spliced> {
    let parsed = shape(body)?;
    let mut out = Spliced {
        continuation: parsed.is_continuation(),
        ..Spliced::default()
    };
    let mut cursor = 0;
    for edit in plan_edits(body, parsed.messages, records) {
        let Some(rebuilt) = with_tail(&body[edit.start..edit.end], edit.tail) else {
            continue;
        };
        out.body.extend_from_slice(&body[cursor..edit.start]);
        out.body.extend_from_slice(&rebuilt);
        cursor = edit.end;
        out.message_indices.push(edit.index);
    }
    out.body.extend_from_slice(&body[cursor..]);
    Some(out)
}
