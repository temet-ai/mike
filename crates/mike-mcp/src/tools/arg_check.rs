use std::collections::HashMap;
use std::sync::OnceLock;

use rmcp::model::{CallToolResult, ErrorData as RmcpError};
use serde_json::{Map, Value, json};

use super::all_tools;
use super::envelope::result_from_envelope;

type Arguments = Option<Map<String, Value>>;

pub(crate) type Corrections = Vec<(String, String)>;

type AliasRules = &'static [(&'static str, &'static str)];

const ALIASES: &[(&str, AliasRules)] = &[
    (
        "find_text",
        &[
            ("query", "needle"),
            ("text", "needle"),
            ("pattern", "needle"),
            ("search", "needle"),
        ],
    ),
    (
        "search_session_content",
        &[
            ("query", "q"),
            ("text", "q"),
            ("needle", "q"),
            ("search", "q"),
            ("project_path", "project"),
        ],
    ),
    (
        "search_project_knowledge",
        &[
            ("q", "query"),
            ("text", "query"),
            ("project_path", "project"),
            ("project_uuid", "project"),
        ],
    ),
    (
        "read_project_knowledge",
        &[
            ("id", "reference"),
            ("ref", "reference"),
            ("uuid", "reference"),
            ("project_path", "project"),
        ],
    ),
    (
        "recall_similar_turns",
        &[
            ("q", "query"),
            ("text", "query"),
            ("project", "project_path"),
        ],
    ),
    (
        "recall_project_context",
        &[("q", "query"), ("project", "project_path")],
    ),
];

const SESSION_TAGGED: [&str; 3] = [
    "search_project_knowledge",
    "recall_similar_turns",
    "recall_project_context",
];

fn passes_through(tool: &str, argument: &str) -> bool {
    argument == "session_id" && SESSION_TAGGED.contains(&tool)
}

struct Declared {
    names: Vec<String>,
    required: Vec<String>,
}

impl Declared {
    fn has(&self, name: &str) -> bool {
        self.names.iter().any(|declared| declared == name)
    }

    fn listing(&self) -> String {
        let required = self
            .required
            .iter()
            .map(|name| format!("{name} (required)"));
        let optional = self
            .names
            .iter()
            .filter(|name| *name != "select" && !self.required.contains(name))
            .cloned();
        required
            .chain(optional)
            .chain(std::iter::once("select".to_string()))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn declared_from(schema: &Map<String, Value>) -> Option<Declared> {
    if schema.get("additionalProperties") != Some(&Value::Bool(false)) {
        return None;
    }
    let names = schema
        .get("properties")?
        .as_object()?
        .keys()
        .cloned()
        .collect();
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|names| {
            names
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    Some(Declared { names, required })
}

fn catalogue() -> &'static HashMap<String, Declared> {
    static CATALOGUE: OnceLock<HashMap<String, Declared>> = OnceLock::new();
    CATALOGUE.get_or_init(|| {
        all_tools()
            .iter()
            .filter_map(|tool| Some((tool.name.to_string(), declared_from(&tool.input_schema)?)))
            .collect()
    })
}

fn rename_aliases(
    name: &str,
    declared: &Declared,
    arguments: &mut Map<String, Value>,
) -> Corrections {
    let rules: AliasRules = ALIASES
        .iter()
        .find(|(tool, _)| *tool == name)
        .map_or(&[], |(_, rules)| *rules);
    let mut corrections = Corrections::new();
    for (alias, target) in rules {
        if declared.has(alias) || arguments.contains_key(*target) {
            continue;
        }
        if let Some(value) = arguments.remove(*alias) {
            arguments.insert((*target).to_string(), value);
            corrections.push(((*alias).to_string(), (*target).to_string()));
        }
    }
    corrections
}

fn reject_unknown(
    name: &str,
    declared: &Declared,
    arguments: &Map<String, Value>,
) -> Result<(), RmcpError> {
    let unknown: Vec<String> = arguments
        .keys()
        .filter(|key| !declared.has(key) && *key != "select" && !passes_through(name, key))
        .map(|key| format!("`{key}`"))
        .collect();
    if unknown.is_empty() {
        return Ok(());
    }
    let noun = if unknown.len() == 1 {
        "argument"
    } else {
        "arguments"
    };
    Err(RmcpError::invalid_params(
        format!(
            "unknown {noun} {} for `{name}`; it takes: {}",
            unknown.join(", "),
            declared.listing()
        ),
        None,
    ))
}

pub(crate) fn check_arguments(
    name: &str,
    arguments: Arguments,
) -> Result<(Arguments, Corrections), RmcpError> {
    let (Some(declared), Some(mut map)) = (catalogue().get(name), arguments.clone()) else {
        return Ok((arguments, Corrections::new()));
    };
    let corrections = rename_aliases(name, declared, &mut map);
    reject_unknown(name, declared, &map)?;
    Ok((Some(map), corrections))
}

pub(crate) fn with_corrections(
    result: CallToolResult,
    corrections: &Corrections,
) -> CallToolResult {
    if corrections.is_empty() || result.is_error == Some(true) {
        return result;
    }
    let Some(Value::Object(mut envelope)) = result.structured_content.clone() else {
        return result;
    };
    let entries = corrections
        .iter()
        .map(|(sent, used)| json!({"sent": sent, "used": used}))
        .collect();
    envelope.insert("arguments_corrected".to_string(), Value::Array(entries));
    result_from_envelope(Value::Object(envelope))
}

#[cfg(test)]
#[path = "arg_check_tests.rs"]
mod tests;
