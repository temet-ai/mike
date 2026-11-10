use serde_json::Value;

use super::select::project_envelope;

const BOOKKEEPING_KEYS: [&str; 4] = [
    "window_note",
    "next_cursor",
    "available_roots",
    "arguments_corrected",
];

pub(super) struct Resolved {
    pub(super) selectors: Vec<String>,
    pub(super) corrected: Vec<(String, String)>,
}

fn is_bookkeeping(key: &str) -> bool {
    BOOKKEEPING_KEYS.contains(&key) || key.starts_with("select_")
}

fn sole_data_key(envelope: &Value) -> Option<&str> {
    let mut keys = envelope
        .as_object()?
        .keys()
        .map(String::as_str)
        .filter(|key| !is_bookkeeping(key));
    let key = keys.next()?;
    keys.next().is_none().then_some(key)
}

fn matches(envelope: &Value, selector: &str) -> bool {
    project_envelope(envelope, &[selector.to_string()])
        .1
        .is_empty()
}

fn under_wrapper(envelope: &Value, wrapper: &str, selector: &str) -> Option<String> {
    let escaped = wrapper.replace('~', "~0").replace('/', "~1");
    let candidate = format!("/{escaped}{selector}");
    matches(envelope, &candidate).then_some(candidate)
}

pub(super) fn resolve_under_wrapper(envelope: &Value, selectors: &[String]) -> Resolved {
    let wrapper = sole_data_key(envelope);
    let mut resolved = Resolved {
        selectors: Vec::with_capacity(selectors.len()),
        corrected: Vec::new(),
    };
    for selector in selectors {
        let retry = wrapper
            .filter(|_| !matches(envelope, selector))
            .and_then(|wrapper| under_wrapper(envelope, wrapper, selector));
        match retry {
            Some(used) => {
                resolved.corrected.push((selector.clone(), used.clone()));
                resolved.selectors.push(used);
            }
            None => resolved.selectors.push(selector.clone()),
        }
    }
    resolved
}

#[cfg(test)]
#[path = "select_fallback_tests.rs"]
mod tests;
