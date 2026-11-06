use serde_json::{Map, Value};

pub(crate) const MAX_STRICT_TOOLS: usize = 1;
pub(crate) const MAX_OPTIONAL_PARAMETERS: usize = 24;

const UNSUPPORTED_KEYWORDS: [&str; 11] = [
    "minLength",
    "maxLength",
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "multipleOf",
    "minItems",
    "maxItems",
    "uniqueItems",
    "propertyNames",
];

const SCHEMA_LISTS: [&str; 3] = ["anyOf", "allOf", "oneOf"];
const SCHEMA_MAPS: [&str; 2] = ["$defs", "definitions"];

pub(crate) fn make_strict(tools: &mut [Value], extended: &[String]) -> Vec<String> {
    let mut candidates: Vec<(usize, usize, usize)> = tools
        .iter()
        .enumerate()
        .filter(|(_, tool)| is_extended(tool, extended) && tool.get("defer_loading").is_none())
        .map(|(index, tool)| (priority(tool), optional_parameters(tool), index))
        .collect();
    candidates.sort_unstable();

    let mut chosen = Vec::new();
    let mut optional_budget = MAX_OPTIONAL_PARAMETERS;
    for (_, optional, index) in candidates {
        if chosen.len() == MAX_STRICT_TOOLS || optional > optional_budget {
            continue;
        }
        optional_budget -= optional;
        chosen.push(index);
    }
    chosen.sort_unstable();

    chosen
        .into_iter()
        .filter_map(|index| {
            let tool = tools.get_mut(index)?;
            if let Some(schema) = tool.get_mut("input_schema") {
                sanitise(schema);
            }
            tool.as_object_mut()?
                .insert("strict".to_owned(), Value::Bool(true));
            tool.get("name").and_then(Value::as_str).map(str::to_owned)
        })
        .collect()
}

fn is_extended(tool: &Value, extended: &[String]) -> bool {
    tool.get("name")
        .and_then(Value::as_str)
        .is_some_and(|name| extended.iter().any(|e| e == name))
}

fn priority(tool: &Value) -> usize {
    usize::from(tool.get("name").and_then(Value::as_str) != Some("Read"))
}

pub(crate) fn optional_parameters(tool: &Value) -> usize {
    let Some(schema) = tool.get("input_schema") else {
        return 0;
    };
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .map_or(0, Map::len);
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    properties.saturating_sub(required)
}

pub(crate) fn sanitise(schema: &mut Value) {
    let Some(node) = schema.as_object_mut() else {
        return;
    };
    for keyword in UNSUPPORTED_KEYWORDS {
        node.remove(keyword);
    }
    let is_object = node.get("type").and_then(Value::as_str) == Some("object")
        || node.contains_key("properties");
    if is_object {
        node.insert("additionalProperties".to_owned(), Value::Bool(false));
    }
    if let Some(Value::Object(properties)) = node.get_mut("properties") {
        properties.values_mut().for_each(sanitise);
    }
    if let Some(items) = node.get_mut("items") {
        sanitise(items);
    }
    for key in SCHEMA_LISTS {
        if let Some(Value::Array(list)) = node.get_mut(key) {
            list.iter_mut().for_each(sanitise);
        }
    }
    for key in SCHEMA_MAPS {
        if let Some(Value::Object(map)) = node.get_mut(key) {
            map.values_mut().for_each(sanitise);
        }
    }
}
