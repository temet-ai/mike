use std::sync::Arc;

use serde::Deserialize;
use serde_json::{Map, Value, json};

pub(super) fn id_only_string_schema() -> Arc<Map<String, Value>> {
    object_schema(
        vec![(
            "id",
            json!({
                "type": "string",
                "description": "Resource identifier."
            }),
        )],
        &["id"],
    )
}

fn select_property() -> Value {
    json!({
        "type": "array",
        "items": {"type": "string"},
        "description": "Optional array of RFC 6901 JSON pointers (e.g. \"/sessions/*/id\") \
            projecting the result to just those parts; \"*\" maps over array elements; omit \
            for the full result. Roots are per tool; a miss returns `available_roots` naming \
            the keys that exist. See server instructions."
    })
}

pub(super) fn object_schema(
    properties: Vec<(&str, Value)>,
    required: &[&str],
) -> Arc<Map<String, Value>> {
    let mut props = Map::new();
    for (name, schema) in properties {
        props.insert(name.to_string(), schema);
    }
    props.insert("select".to_string(), select_property());
    let mut schema = Map::new();
    schema.insert("type".to_string(), Value::String("object".to_string()));
    schema.insert("properties".to_string(), Value::Object(props));
    schema.insert("additionalProperties".to_string(), Value::Bool(false));
    if !required.is_empty() {
        schema.insert(
            "required".to_string(),
            Value::Array(
                required
                    .iter()
                    .map(|s| Value::String(s.to_string()))
                    .collect(),
            ),
        );
    }
    Arc::new(schema)
}

pub(super) fn order_property() -> Value {
    json!({
        "type": "string",
        "enum": ["asc", "desc"],
        "description": "Sort direction: `desc` (default, newest-first) or `asc`."
    })
}

pub(super) fn id_only_schema() -> Arc<Map<String, Value>> {
    object_schema(
        vec![(
            "id",
            json!({
                "type": "integer",
                "minimum": 0,
                "description": "The numeric request id."
            }),
        )],
        &["id"],
    )
}

pub(super) fn id_with_pointer_schema() -> Arc<Map<String, Value>> {
    object_schema(
        vec![
            (
                "id",
                json!({
                    "type": "integer",
                    "minimum": 0,
                    "description": "The numeric request id."
                }),
            ),
            (
                "pointer",
                json!({
                    "type": "string",
                    "description": "Optional RFC 6901 JSON pointer (e.g. \"/system/2/text\"). Returns the FULL selected subtree as pretty-printed JSON. Omit for the full body."
                }),
            ),
        ],
        &["id"],
    )
}

#[derive(Debug, Deserialize)]
pub(crate) struct IdStringArgs {
    pub(crate) id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UpdateByIdArgs {
    pub(crate) id: String,
    #[serde(flatten)]
    pub(crate) rest: Map<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_by_id_args_flatten_keeps_sent_fields_only() -> anyhow::Result<()> {
        let args: UpdateByIdArgs = serde_json::from_value(json!({"id": "58", "mode": "observe"}))?;
        assert_eq!(args.id, "58");
        assert_eq!(args.rest.len(), 1, "rest must carry only the sent field(s)");
        assert_eq!(
            args.rest.get("mode"),
            Some(&Value::String("observe".to_string()))
        );
        assert!(
            !args.rest.contains_key("label") && !args.rest.contains_key("blocks"),
            "a mode-only call must not synthesize unrelated keys"
        );
        Ok(())
    }

    #[test]
    fn object_schema_always_advertises_select() {
        let schema = object_schema(vec![], &[]);
        let props = schema.get("properties").and_then(Value::as_object);
        assert!(
            props.is_some_and(|p| p.get("select").and_then(Value::as_object).is_some()),
            "object_schema must include a `select` property"
        );
    }
}
