use std::borrow::Cow;

use serde_json::Value as JsonValue;

use crate::store::models::InterceptedRequest;

pub(crate) const NUL_STAND_IN: &str = "\u{2400}";

const NUL_ESCAPE: &[u8] = b"\\u0000";

pub(crate) fn has_nul(text: &str) -> bool {
    text.contains('\0')
}

pub(crate) fn clean_text(text: &str) -> Cow<'_, str> {
    if has_nul(text) {
        Cow::Owned(text.replace('\0', NUL_STAND_IN))
    } else {
        Cow::Borrowed(text)
    }
}

pub(crate) fn clean_json(value: &mut JsonValue) -> bool {
    match value {
        JsonValue::String(text) => clean_string(text),
        JsonValue::Array(items) => items
            .iter_mut()
            .fold(false, |changed, item| clean_json(item) | changed),
        JsonValue::Object(map) => clean_object(map),
        _ => false,
    }
}

pub(crate) fn cleaned_json(value: &JsonValue) -> Cow<'_, JsonValue> {
    let mut copy = value.clone();
    if clean_json(&mut copy) {
        Cow::Owned(copy)
    } else {
        Cow::Borrowed(value)
    }
}

pub(crate) fn body_carries_nul(bytes: &[u8]) -> bool {
    bytes.contains(&0) || bytes.windows(NUL_ESCAPE.len()).any(|w| w == NUL_ESCAPE)
}

pub(crate) fn request_with_clean_text(req: &InterceptedRequest) -> Cow<'_, InterceptedRequest> {
    if !has_nul(&req.human_prompt) && !has_nul(&req.last_user_message) {
        return Cow::Borrowed(req);
    }
    let mut owned = req.clone();
    owned.human_prompt = clean_text(&req.human_prompt).into_owned();
    owned.last_user_message = clean_text(&req.last_user_message).into_owned();
    Cow::Owned(owned)
}

fn clean_string(text: &mut String) -> bool {
    if !has_nul(text) {
        return false;
    }
    *text = text.replace('\0', NUL_STAND_IN);
    true
}

fn clean_object(map: &mut serde_json::Map<String, JsonValue>) -> bool {
    let mut changed = false;
    if map.keys().any(|key| has_nul(key)) {
        let old = std::mem::take(map);
        for (key, value) in old {
            map.insert(key.replace('\0', NUL_STAND_IN), value);
        }
        changed = true;
    }
    for value in map.values_mut() {
        changed |= clean_json(value);
    }
    changed
}

#[cfg(test)]
#[path = "text_sanitize_tests.rs"]
mod text_sanitize_tests;
