use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{
    IdStringArgs, UpdateByIdArgs, arguments_as_value, id_only_string_schema, object_schema,
    parse_args_default, parse_args_required, wrap_success,
};
use crate::client::InspectClient;
use crate::error::McpError;

pub const PROMPTS_TOOL_NAMES: &[&str] = &[
    TOOL_LIST_PROMPT_COMPOSITIONS,
    TOOL_GET_PROMPT_COMPOSITION,
    TOOL_CREATE_PROMPT_COMPOSITION,
    TOOL_UPDATE_PROMPT_COMPOSITION,
    TOOL_DELETE_PROMPT_COMPOSITION,
    TOOL_LIST_PROMPT_ANOMALIES,
];

pub const TOOL_LIST_PROMPT_COMPOSITIONS: &str = "list_prompt_compositions";
pub const TOOL_GET_PROMPT_COMPOSITION: &str = "get_prompt_composition";
pub const TOOL_CREATE_PROMPT_COMPOSITION: &str = "create_prompt_composition";
pub const TOOL_UPDATE_PROMPT_COMPOSITION: &str = "update_prompt_composition";
pub const TOOL_DELETE_PROMPT_COMPOSITION: &str = "delete_prompt_composition";
pub const TOOL_LIST_PROMPT_ANOMALIES: &str = "list_prompt_anomalies";

pub fn prompts_tools() -> Vec<Tool> {
    vec![
        list_prompt_compositions_tool(),
        get_prompt_composition_tool(),
        create_prompt_composition_tool(),
        update_prompt_composition_tool(),
        delete_prompt_composition_tool(),
        list_prompt_anomalies_tool(),
    ]
}

pub fn owns(name: &str) -> bool {
    PROMPTS_TOOL_NAMES.contains(&name)
}

fn list_prompt_compositions_tool() -> Tool {
    Tool::new(
        Cow::Borrowed(TOOL_LIST_PROMPT_COMPOSITIONS),
        Cow::Borrowed(
            "List registered prompt compositions without per-block bodies. \
             Returns an array of `{id, prompt_sha256, label, version, mode, block_count, total_chars, applied_count_24h, updated_at}`.",
        ),
        object_schema(vec![], &[]),
    )
}

fn get_prompt_composition_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "id",
                json!({
                    "type": "string",
                    "description": "Surrogate composition id (integer as string)."
                }),
            ),
            (
                "sha256",
                json!({
                    "type": "string",
                    "description": "Canonical sha256 hex (64 chars) — the deep-link key from request audit."
                }),
            ),
        ],
        &[],
    );
    Tool::new(
        Cow::Borrowed(TOOL_GET_PROMPT_COMPOSITION),
        Cow::Borrowed(
            "Fetch one composition by exactly one of `id` or `sha256`; both or neither is an \
             error. 404 when no composition matches.",
        ),
        schema,
    )
}

fn create_prompt_composition_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "prompt_sha256",
                json!({
                    "type": "string",
                    "description": "Canonical sha256 hex (must not be empty)."
                }),
            ),
            (
                "label",
                json!({
                    "type": "string",
                    "description": "Operator-facing label (must not be empty)."
                }),
            ),
            (
                "blocks",
                json!({
                    "type": "array",
                    "description": "Ordered block bodies. Each item is `{text: string, cache_control?: any}`.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "text": { "type": "string" },
                            "cache_control": {}
                        },
                        "required": ["text"]
                    }
                }),
            ),
        ],
        &["prompt_sha256", "label", "blocks"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_CREATE_PROMPT_COMPOSITION),
        Cow::Borrowed(
            "Create a composition. The server assigns `id`, `version` 1 and the timestamps. \
             Returns `{composition, warnings}`. 400 on validator rejection; 409 on `prompt_sha256` collision.",
        ),
        schema,
    )
}

fn update_prompt_composition_tool() -> Tool {
    let schema = object_schema(
        vec![
            ("id", json!({ "type": "string" })),
            ("label", json!({ "type": "string" })),
            (
                "blocks",
                json!({
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "text": { "type": "string" },
                            "cache_control": {}
                        },
                        "required": ["text"]
                    }
                }),
            ),
            (
                "mode",
                json!({
                    "type": "string",
                    "enum": ["observe", "template", "replace"]
                }),
            ),
            (
                "notes",
                json!({
                    "type": ["string", "null"],
                    "description": "Operator notes. Omit to leave unchanged; send `null` to clear."
                }),
            ),
        ],
        &["id"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_UPDATE_PROMPT_COMPOSITION),
        Cow::Borrowed(
            "Patch a composition; pass at least one of `label`, `blocks`, `mode` or `notes`. \
             Bumps `version`. 400 on empty body; 404 on unknown id.",
        ),
        schema,
    )
}

fn delete_prompt_composition_tool() -> Tool {
    Tool::new(
        Cow::Borrowed(TOOL_DELETE_PROMPT_COMPOSITION),
        Cow::Borrowed(
            "Delete a composition by id. Returns `{deleted: true, warnings: [...]}`. \
             404 when id is unknown.",
        ),
        id_only_string_schema(),
    )
}

fn list_prompt_anomalies_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "date_from",
                json!({
                    "type": "string",
                    "description": "RFC 3339 lower bound on `requests.created_at` (inclusive). Absent → 7-day default window."
                }),
            ),
            (
                "date_to",
                json!({
                    "type": "string",
                    "description": "RFC 3339 upper bound on `requests.created_at` (exclusive). Absent → now (paired with 7-day default)."
                }),
            ),
            (
                "limit",
                json!({
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 500,
                    "description": "Maximum anomaly groups per page (default 200, max 500)."
                }),
            ),
            (
                "offset",
                json!({
                    "type": "integer",
                    "minimum": 0,
                    "description": "Skip this many anomaly groups before returning. Default 0."
                }),
            ),
        ],
        &[],
    );
    Tool::new(
        Cow::Borrowed(TOOL_LIST_PROMPT_ANOMALIES),
        Cow::Borrowed(
            "List prompt-composition anomalies grouped by `classification`, `prompt_sha256` and \
             `signature_hash`. Each group: `count`, `observations` (one `{request_id, session_id}` \
             pair per occurrence), `first_seen`, `last_seen`. `classification` is `unknown-class` \
             or `structural-mismatch`. Pass `date_from`, `date_to`, `limit` and `offset` for a \
             wider or paginated view.",
        ),
        schema,
    )
}

#[derive(Debug, Deserialize)]
pub(crate) struct GetPromptCompositionArgs {
    #[serde(default)]
    pub(crate) id: Option<String>,
    #[serde(default)]
    pub(crate) sha256: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct ListPromptAnomaliesArgs {
    #[serde(default)]
    pub(crate) date_from: Option<String>,
    #[serde(default)]
    pub(crate) date_to: Option<String>,
    #[serde(default)]
    pub(crate) limit: Option<u64>,
    #[serde(default)]
    pub(crate) offset: Option<u64>,
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    match name {
        TOOL_LIST_PROMPT_COMPOSITIONS => {
            let value = client
                .list_prompt_compositions()
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("compositions", value))
        }
        TOOL_GET_PROMPT_COMPOSITION => {
            let args = parse_args_required::<GetPromptCompositionArgs>(arguments)?;
            let value = match (args.id.as_deref(), args.sha256.as_deref()) {
                (Some(_), Some(_)) => {
                    return Err(RmcpError::invalid_params(
                        "supply exactly one of `id` or `sha256`, not both",
                        None,
                    ));
                }
                (None, None) => {
                    return Err(RmcpError::invalid_params(
                        "one of `id` or `sha256` is required",
                        None,
                    ));
                }
                (Some(id), None) => client
                    .get_prompt_composition(id)
                    .await
                    .map_err(McpError::into_rmcp)?,
                (None, Some(sha256)) => client
                    .get_prompt_composition_by_sha256(sha256)
                    .await
                    .map_err(McpError::into_rmcp)?,
            };
            Ok(wrap_success("composition", value))
        }
        TOOL_CREATE_PROMPT_COMPOSITION => {
            let body = arguments_as_value(arguments);
            let value = client
                .create_prompt_composition(body)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("composition", value))
        }
        TOOL_UPDATE_PROMPT_COMPOSITION => {
            let args = parse_args_required::<UpdateByIdArgs>(arguments)?;
            let value = client
                .update_prompt_composition(&args.id, Value::Object(args.rest))
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("composition", value))
        }
        TOOL_DELETE_PROMPT_COMPOSITION => {
            let args = parse_args_required::<IdStringArgs>(arguments)?;
            let value = client
                .delete_prompt_composition(&args.id)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("result", value))
        }
        TOOL_LIST_PROMPT_ANOMALIES => {
            let args = parse_args_default::<ListPromptAnomaliesArgs>(arguments)?;
            let mut q: Vec<(&str, String)> = Vec::new();
            if let Some(ref df) = args.date_from {
                q.push(("date_from", df.clone()));
            }
            if let Some(ref dt) = args.date_to {
                q.push(("date_to", dt.clone()));
            }
            if let Some(l) = args.limit {
                q.push(("limit", l.to_string()));
            }
            if let Some(o) = args.offset {
                q.push(("offset", o.to_string()));
            }
            let value = client
                .list_prompt_anomalies(&q)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("anomalies", value))
        }
        other => Err(RmcpError::invalid_params(
            format!("unknown prompt composition tool: {other}"),
            None,
        )),
    }
}
