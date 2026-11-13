use std::borrow::Cow;
use std::hash::{DefaultHasher, Hash, Hasher};

use serde::Deserialize;
use serde_json::Value;
use serde_json::value::RawValue;

const PLAIN_STRING_OPEN: &[u8] = br#"[{"type":"text","text":"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentShape {
    Blocks,
    PlainString,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InjectedBlocks {
    pub index: usize,
    identity: u64,
    shape: ContentShape,
    tail: Vec<u8>,
}

#[derive(Deserialize)]
struct MessagesShape<'a> {
    #[serde(borrow, default)]
    messages: Vec<&'a RawValue>,
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

fn messages(body: &[u8]) -> Option<Vec<&RawValue>> {
    serde_json::from_slice::<MessagesShape<'_>>(body)
        .ok()
        .map(|m| m.messages)
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

fn identity(message: &RawValue) -> Option<u64> {
    let mut value: Value = serde_json::from_str(message.get()).ok()?;
    strip_cache_control(&mut value);
    let canonical = serde_json::to_vec(&value).ok()?;
    let mut hasher = DefaultHasher::new();
    canonical.hash(&mut hasher);
    Some(hasher.finish())
}

fn plain_string_prefix(original: &[u8]) -> Vec<u8> {
    [PLAIN_STRING_OPEN, original, b"}"].concat()
}

fn appended_tail(original: &[u8], injected: &[u8]) -> Option<(ContentShape, Vec<u8>)> {
    match original.first()? {
        b'[' => {
            let open = original.len().checked_sub(1)?;
            let grew = array_len(injected)? > array_len(original)?;
            (original.last() == Some(&b']')
                && grew
                && injected.len() > original.len()
                && injected.get(..open)? == original.get(..open)?)
            .then(|| (ContentShape::Blocks, injected[open..].to_vec()))
        }
        b'"' => {
            let prefix = plain_string_prefix(original);
            let tail = injected.strip_prefix(prefix.as_slice())?;
            (tail.first() == Some(&b',')).then(|| (ContentShape::PlainString, tail.to_vec()))
        }
        _ => None,
    }
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
    let (shape, tail) = appended_tail(&original[os..oe], &injected[is..ie])?;
    Some(InjectedBlocks {
        index,
        identity: identity(before[index])?,
        shape,
        tail,
    })
}

pub fn splice(body: &[u8], record: &InjectedBlocks) -> Option<Vec<u8>> {
    let all = messages(body)?;
    let message = all.get(record.index)?;
    if identity(message)? != record.identity {
        return None;
    }
    let (start, end) = content_span(body, message)?;
    let content = &body[start..end];
    let rebuilt = match (record.shape, content.first()?) {
        (ContentShape::Blocks, b'[') if content.last() == Some(&b']') => {
            [&content[..content.len() - 1], record.tail.as_slice()].concat()
        }
        (ContentShape::PlainString, b'"') => [
            plain_string_prefix(content).as_slice(),
            record.tail.as_slice(),
        ]
        .concat(),
        _ => return None,
    };
    Some([&body[..start], rebuilt.as_slice(), &body[end..]].concat())
}
