use serde_json::{Map, Value, json};

use super::select::{Segment, parse_pointer};

fn escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

fn step<'a>(node: &'a Value, segment: &Segment) -> Option<(&'a Value, String)> {
    match (segment, node) {
        (Segment::Key(key), Value::Object(map)) => map.get(key).map(|c| (c, escape(key))),
        (Segment::Key(key), Value::Array(arr)) => {
            let idx: usize = key.parse().ok()?;
            arr.get(idx).map(|c| (c, key.clone()))
        }
        (Segment::Wildcard, Value::Array(arr)) => arr.first().map(|c| (c, "*".to_string())),
        (Segment::Wildcard, Value::Object(map)) => {
            map.values().next().map(|c| (c, "*".to_string()))
        }
        _ => None,
    }
}

fn child_keys(node: &Value) -> Vec<Value> {
    match node {
        Value::Object(map) => map.keys().cloned().map(Value::String).collect(),
        Value::Array(arr) if !arr.is_empty() => vec![Value::String("*".to_string())],
        _ => Vec::new(),
    }
}

fn hint_for(envelope: &Value, selector: &str) -> Value {
    let mut node = envelope;
    let mut prefix = String::new();
    for segment in parse_pointer(selector) {
        let Some((child, token)) = step(node, &segment) else {
            break;
        };
        node = child;
        prefix.push('/');
        prefix.push_str(&token);
    }
    json!({"prefix": prefix, "keys": child_keys(node)})
}

pub(super) fn available_roots(envelope: &Value, unmatched: &[String]) -> Value {
    let map: Map<String, Value> = unmatched
        .iter()
        .map(|selector| (selector.clone(), hint_for(envelope, selector)))
        .collect();
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots(envelope: &Value, selector: &str) -> Value {
        available_roots(envelope, &[selector.to_string()])[selector].clone()
    }

    #[test]
    fn miss_at_the_root_lists_top_level_keys() {
        let envelope = json!({"roster": {"roster": [{"session_id": "a"}]}});
        assert_eq!(
            roots(&envelope, "/sessions/*/id"),
            json!({"prefix": "", "keys": ["roster"]})
        );
    }

    #[test]
    fn miss_below_a_wildcard_lists_the_first_element_keys() {
        let envelope = json!({"mailbox": {"inbox": [{"message_uuid": "m", "body": "b"}]}});
        assert_eq!(
            roots(&envelope, "/mailbox/inbox/*/id"),
            json!({"prefix": "/mailbox/inbox/*", "keys": ["body", "message_uuid"]})
        );
    }

    #[test]
    fn miss_on_an_array_suggests_the_wildcard() {
        let envelope = json!({"requests": [{"id": 1}]});
        assert_eq!(
            roots(&envelope, "/requests/id"),
            json!({"prefix": "/requests", "keys": ["*"]})
        );
    }

    #[test]
    fn empty_array_and_scalar_report_no_keys() {
        let envelope = json!({"rows": [], "count": 0});
        assert_eq!(
            roots(&envelope, "/rows/*/id"),
            json!({"prefix": "/rows", "keys": []})
        );
        assert_eq!(
            roots(&envelope, "/count/x"),
            json!({"prefix": "/count", "keys": []})
        );
    }

    #[test]
    fn escaped_keys_round_trip_into_the_prefix() {
        let envelope = json!({"a/b": {"c": 1}});
        assert_eq!(
            roots(&envelope, "/a~1b/d"),
            json!({"prefix": "/a~1b", "keys": ["c"]})
        );
    }
}
