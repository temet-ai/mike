use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde_json::{Map, Value, json};

use super::{object_schema, wrap_success};
use crate::client::InspectClient;
use crate::error::McpError;

pub const TOOL_GET_SEMANTIC_STATUS: &str = "get_semantic_status";
pub const TOOL_GET_SEMANTIC_NEIGHBOURS: &str = "get_semantic_neighbours";
pub const TOOL_GET_SEMANTIC_ITEM: &str = "get_semantic_item";
pub const TOOL_LIST_PATTERN_ASSIGNMENTS: &str = "list_pattern_assignments";
pub const TOOL_LIST_PATTERN_CANDIDATES: &str = "list_pattern_candidates";
pub const TOOL_GET_EMBEDDING_DRIFT: &str = "get_embedding_drift";

pub const SEMANTIC_TOOL_NAMES: &[&str] = &[
    TOOL_GET_SEMANTIC_STATUS,
    TOOL_GET_SEMANTIC_NEIGHBOURS,
    TOOL_GET_SEMANTIC_ITEM,
    TOOL_LIST_PATTERN_ASSIGNMENTS,
    TOOL_LIST_PATTERN_CANDIDATES,
    TOOL_GET_EMBEDDING_DRIFT,
];

const FLOOR_NOTE: &str = "Score is similarity minus the chance floor for the facet pair, and only items above \
     the floor are kept. Every text is the operator's words, verbatim and whole.";

pub fn owns(name: &str) -> bool {
    SEMANTIC_TOOL_NAMES.contains(&name)
}

fn string(desc: &str) -> Value {
    json!({"type": "string", "description": desc})
}

fn integer(desc: &str) -> Value {
    json!({"type": "integer", "description": desc})
}

fn tool(
    name: &'static str,
    desc: String,
    props: Vec<(&'static str, Value)>,
    required: &[&'static str],
) -> Tool {
    Tool::new(
        Cow::Borrowed(name),
        Cow::Owned(desc),
        object_schema(props, required),
    )
}

pub fn semantic_tools() -> Vec<Tool> {
    vec![
        tool(
            TOOL_GET_SEMANTIC_STATUS,
            format!(
                "Semantic context status: the toggle, whether embedding stays on the machine, the current embedder, the chance floors in force, item counts, backlog and the worker's last tick. {FLOOR_NOTE}"
            ),
            vec![],
            &[],
        ),
        tool(
            TOOL_GET_SEMANTIC_NEIGHBOURS,
            format!(
                "Earlier corrections, exchanges, directives or lessons that resemble a closed turn, as of a moment, across sessions and projects. {FLOOR_NOTE}"
            ),
            vec![
                ("turn_id", integer("memory.operator_turns id.")),
                (
                    "audience",
                    string("judge, steerer (default), label_free or internal."),
                ),
                (
                    "kinds",
                    string(
                        "Comma list of correction, exchange, directive, lesson; defaults to what the audience may see.",
                    ),
                ),
                ("scope", string("operator, project or session.")),
                ("as_of", string("RFC 3339; defaults to the turn's close.")),
                (
                    "as_of_mode",
                    string("knowledge (default) or reconstructed."),
                ),
                ("k", integer("Neighbours to return.")),
                ("offset", integer("Neighbours to skip.")),
            ],
            &["turn_id"],
        ),
        tool(
            TOOL_GET_SEMANTIC_ITEM,
            format!(
                "One semantic item whole, including its 768-float embedding and data. {FLOOR_NOTE}"
            ),
            vec![("id", integer("memory.semantic_items id."))],
            &["id"],
        ),
        tool(
            TOOL_LIST_PATTERN_ASSIGNMENTS,
            format!(
                "The pattern tagger's output: each qualified correction with its nearest audited pattern, or unassigned below the minimum score, with the top three anchors. Never labels. {FLOOR_NOTE}"
            ),
            vec![
                ("from", string("RFC 3339.")),
                ("to", string("RFC 3339.")),
                ("pattern", string("One pattern slug.")),
                (
                    "unassigned",
                    json!({"type": "boolean", "description": "Only unassigned rows."}),
                ),
                ("limit", integer("Rows per page, 1 to 500.")),
                ("offset", integer("Rows to skip.")),
            ],
            &[],
        ),
        tool(
            TOOL_LIST_PATTERN_CANDIDATES,
            format!(
                "Groups of untagged corrections proposed as new failure patterns, with every member listed. Naming and dismissing are the operator's, in the SPA. {FLOOR_NOTE}"
            ),
            vec![
                ("status", string("open, named, dismissed or superseded.")),
                ("limit", integer("Rows per page, 1 to 500.")),
                ("offset", integer("Rows to skip.")),
            ],
            &[],
        ),
        tool(
            TOOL_GET_EMBEDDING_DRIFT,
            format!(
                "Daily embedding drift of the operator's prompts, turn endings and corrections, with the embedding_drift alarm episodes. {FLOOR_NOTE}"
            ),
            vec![
                ("stream", string("prompt, ending or correction.")),
                ("from", string("YYYY-MM-DD.")),
                ("to", string("YYYY-MM-DD.")),
            ],
            &[],
        ),
    ]
}

fn scalar_text(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn pairs(
    arguments: Option<&Map<String, Value>>,
    allowed: &[&'static str],
    skip: &[&str],
) -> Result<Vec<(&'static str, String)>, RmcpError> {
    let Some(arguments) = arguments else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for (key, value) in arguments
        .iter()
        .filter(|(k, _)| !skip.contains(&k.as_str()))
    {
        let name = allowed
            .iter()
            .find(|a| **a == key.as_str())
            .ok_or_else(|| RmcpError::invalid_params(format!("unknown argument {key:?}"), None))?;
        let text = scalar_text(value).ok_or_else(|| {
            RmcpError::invalid_params(
                format!("argument {key:?} must be a scalar, got {value}"),
                None,
            )
        })?;
        out.push((*name, text));
    }
    Ok(out)
}

fn id_arg(arguments: Option<&Map<String, Value>>, key: &str) -> Result<i64, RmcpError> {
    arguments
        .and_then(|a| a.get(key))
        .and_then(Value::as_i64)
        .ok_or_else(|| RmcpError::invalid_params(format!("{key} (integer) is required"), None))
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    let args = arguments.as_ref();
    let value = match name {
        TOOL_GET_SEMANTIC_STATUS => client.get_semantic_status().await,
        TOOL_GET_SEMANTIC_NEIGHBOURS => {
            let turn_id = id_arg(args, "turn_id")?;
            let allowed = [
                "audience",
                "kinds",
                "scope",
                "as_of",
                "as_of_mode",
                "k",
                "offset",
            ];
            let query = pairs(args, &allowed, &["turn_id"])?;
            client.get_semantic_neighbours(turn_id, &query).await
        }
        TOOL_GET_SEMANTIC_ITEM => client.get_semantic_item(id_arg(args, "id")?).await,
        TOOL_LIST_PATTERN_ASSIGNMENTS => {
            let allowed = ["from", "to", "pattern", "unassigned", "limit", "offset"];
            client
                .list_pattern_assignments(&pairs(args, &allowed, &[])?)
                .await
        }
        TOOL_LIST_PATTERN_CANDIDATES => {
            client
                .list_pattern_candidates(&pairs(args, &["status", "limit", "offset"], &[])?)
                .await
        }
        TOOL_GET_EMBEDDING_DRIFT => {
            client
                .get_embedding_drift(&pairs(args, &["stream", "from", "to"], &[])?)
                .await
        }
        _ => {
            return Err(RmcpError::invalid_params(
                format!("unknown semantic tool: {name}"),
                None,
            ));
        }
    }
    .map_err(McpError::into_rmcp)?;
    Ok(wrap_success(name, value))
}

#[cfg(test)]
mod tests {
    use super::{SEMANTIC_TOOL_NAMES, owns, semantic_tools};

    #[test]
    fn every_semantic_tool_is_owned_and_described_with_the_floor() {
        let tools = semantic_tools();
        assert_eq!(tools.len(), SEMANTIC_TOOL_NAMES.len());
        for t in &tools {
            assert!(owns(t.name.as_ref()));
            let desc = t.description.as_deref().unwrap_or_default();
            assert!(desc.contains("chance floor"), "{}", t.name);
        }
        assert!(!owns("name_pattern_candidate"));
    }
}
