use std::collections::BTreeMap;

use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Segment {
    Key(String),
    Wildcard,
}

pub(super) fn parse_pointer(selector: &str) -> Vec<Segment> {
    if selector.is_empty() {
        return Vec::new();
    }
    selector[1..]
        .split('/')
        .map(|raw| {
            if raw == "*" {
                Segment::Wildcard
            } else {
                Segment::Key(raw.replace("~1", "/").replace("~0", "~"))
            }
        })
        .collect()
}

enum Proj {
    Leaf(Value),
    Object(BTreeMap<String, Proj>),
    Array(BTreeMap<usize, Proj>),
}

impl Proj {
    fn into_value(self) -> Value {
        match self {
            Proj::Leaf(v) => v,
            Proj::Object(map) => {
                Value::Object(map.into_iter().map(|(k, v)| (k, v.into_value())).collect())
            }
            Proj::Array(map) => Value::Array(map.into_values().map(Proj::into_value).collect()),
        }
    }

    fn merge(&mut self, other: Proj) {
        match (&mut *self, other) {
            (Proj::Leaf(_), _) => {}
            (slot, other @ Proj::Leaf(_)) => *slot = other,
            (Proj::Object(a), Proj::Object(b)) => {
                for (k, v) in b {
                    match a.get_mut(&k) {
                        Some(existing) => existing.merge(v),
                        None => {
                            a.insert(k, v);
                        }
                    }
                }
            }
            (Proj::Array(a), Proj::Array(b)) => {
                for (k, v) in b {
                    match a.get_mut(&k) {
                        Some(existing) => existing.merge(v),
                        None => {
                            a.insert(k, v);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn select(source: &Value, segments: &[Segment]) -> Option<Proj> {
    match segments.split_first() {
        None => Some(Proj::Leaf(source.clone())),
        Some((Segment::Key(key), rest)) => match source {
            Value::Object(map) => {
                let child = map.get(key)?;
                let proj = select(child, rest)?;
                Some(Proj::Object(BTreeMap::from([(key.clone(), proj)])))
            }
            Value::Array(arr) => {
                let idx: usize = key.parse().ok()?;
                let child = arr.get(idx)?;
                let proj = select(child, rest)?;
                Some(Proj::Array(BTreeMap::from([(idx, proj)])))
            }
            _ => None,
        },
        Some((Segment::Wildcard, rest)) => match source {
            Value::Array(arr) => {
                let mapped: BTreeMap<usize, Proj> = arr
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, child)| select(child, rest).map(|proj| (idx, proj)))
                    .collect();
                (!mapped.is_empty()).then_some(Proj::Array(mapped))
            }
            Value::Object(map) => {
                let mapped: BTreeMap<String, Proj> = map
                    .iter()
                    .filter_map(|(key, child)| select(child, rest).map(|proj| (key.clone(), proj)))
                    .collect();
                (!mapped.is_empty()).then_some(Proj::Object(mapped))
            }
            _ => None,
        },
    }
}

pub(super) fn project_envelope(envelope: &Value, selectors: &[String]) -> (Value, Vec<String>) {
    if selectors.is_empty() {
        return (envelope.clone(), Vec::new());
    }
    let mut merged: Option<Proj> = None;
    let mut unmatched = Vec::new();
    for selector in selectors {
        let segments = parse_pointer(selector);
        match select(envelope, &segments) {
            Some(proj) => match &mut merged {
                Some(existing) => existing.merge(proj),
                None => merged = Some(proj),
            },
            None => unmatched.push(selector.clone()),
        }
    }
    let value = merged
        .map(Proj::into_value)
        .unwrap_or_else(|| Value::Object(Map::new()));
    (value, unmatched)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn project_envelope_single_pointer_keeps_subtree_whole() {
        let envelope = json!({
            "request": {
                "id": 42,
                "body": {"a": 1, "b": 2},
                "other": "ignored"
            }
        });
        let (projected, unmatched) = project_envelope(&envelope, &["/request/body".to_string()]);
        assert_eq!(projected, json!({"request": {"body": {"a": 1, "b": 2}}}));
        assert!(unmatched.is_empty());
    }

    #[test]
    fn project_envelope_wildcard_projects_field_from_every_array_element() {
        let envelope = json!({
            "sessions": [
                {"id": "a", "extra": "x"},
                {"id": "b", "extra": "y"}
            ]
        });
        let (projected, unmatched) = project_envelope(&envelope, &["/sessions/*/id".to_string()]);
        assert_eq!(projected, json!({"sessions": [{"id": "a"}, {"id": "b"}]}));
        assert!(unmatched.is_empty());
    }

    #[test]
    fn project_envelope_multiple_selectors_union() {
        let envelope = json!({
            "request": {"id": 1, "path": "/v1/messages", "status": 200}
        });
        let (projected, unmatched) = project_envelope(
            &envelope,
            &["/request/id".to_string(), "/request/status".to_string()],
        );
        assert_eq!(projected, json!({"request": {"id": 1, "status": 200}}));
        assert!(unmatched.is_empty());
    }

    #[test]
    fn project_envelope_unescapes_tilde_and_slash_segments() {
        let envelope = json!({
            "a/b": {"c~d": "value"}
        });
        let (projected, unmatched) = project_envelope(&envelope, &["/a~1b/c~0d".to_string()]);
        assert_eq!(projected, json!({"a/b": {"c~d": "value"}}));
        assert!(unmatched.is_empty());
    }

    #[test]
    fn project_envelope_unmatched_selector_reported_not_error() {
        let envelope = json!({"request": {"id": 1}});
        let (projected, unmatched) = project_envelope(&envelope, &["/request/nope".to_string()]);
        assert_eq!(projected, json!({}));
        assert_eq!(unmatched, vec!["/request/nope".to_string()]);
    }

    #[test]
    fn project_envelope_empty_selectors_is_identity_passthrough() {
        let envelope = json!({"request": {"id": 1, "path": "/v1/messages"}});
        let (projected, unmatched) = project_envelope(&envelope, &[]);
        assert_eq!(projected, envelope);
        assert!(unmatched.is_empty());
    }

    #[test]
    fn project_envelope_root_pointer_selects_whole_document() {
        let envelope = json!({"request": {"id": 1}});
        let (projected, unmatched) = project_envelope(&envelope, &[String::new()]);
        assert_eq!(projected, envelope);
        assert!(unmatched.is_empty());
    }

    #[test]
    fn project_envelope_nested_wildcard_maps_both_levels() {
        let envelope = json!({
            "a": [
                {"b": [{"c": 1, "d": "x"}, {"c": 2, "d": "y"}]},
                {"b": [{"c": 3, "d": "z"}]}
            ]
        });
        let (projected, unmatched) = project_envelope(&envelope, &["/a/*/b/*/c".to_string()]);
        assert_eq!(
            projected,
            json!({
                "a": [
                    {"b": [{"c": 1}, {"c": 2}]},
                    {"b": [{"c": 3}]}
                ]
            })
        );
        assert!(unmatched.is_empty());
    }

    #[test]
    fn project_envelope_wildcard_maps_over_object_values() {
        let envelope = json!({
            "by_status": {
                "200": {"count": 5, "note": "ok"},
                "500": {"count": 1, "note": "bad"}
            }
        });
        let (projected, unmatched) =
            project_envelope(&envelope, &["/by_status/*/count".to_string()]);
        assert_eq!(
            projected,
            json!({
                "by_status": {
                    "200": {"count": 5},
                    "500": {"count": 1}
                }
            })
        );
        assert!(unmatched.is_empty());
    }

    #[test]
    fn project_envelope_broad_selector_wins_over_narrow_at_same_path_either_order() {
        let envelope = json!({
            "request": {"id": 1, "path": "/v1/messages", "status": 200}
        });
        let whole = json!({"request": {"id": 1, "path": "/v1/messages", "status": 200}});
        let (narrow_first, _) = project_envelope(
            &envelope,
            &["/request/id".to_string(), "/request".to_string()],
        );
        assert_eq!(narrow_first, whole);
        let (broad_first, _) = project_envelope(
            &envelope,
            &["/request".to_string(), "/request/id".to_string()],
        );
        assert_eq!(broad_first, narrow_first);
    }
}
