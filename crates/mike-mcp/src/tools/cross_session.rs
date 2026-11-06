use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde_json::{Map, Value, json};

use super::{object_schema, wrap_success};
use crate::client::InspectClient;
use crate::error::McpError;

pub const TOOL_GET_CROSS_SESSION_CALIBRATION: &str = "get_cross_session_calibration";
pub const TOOL_LIST_DIRECTIVE_PROPOSALS: &str = "list_directive_proposals";

pub const CROSS_SESSION_TOOL_NAMES: &[&str] = &[
    TOOL_GET_CROSS_SESSION_CALIBRATION,
    TOOL_LIST_DIRECTIVE_PROPOSALS,
];

pub fn owns(name: &str) -> bool {
    CROSS_SESSION_TOOL_NAMES.contains(&name)
}

pub fn cross_session_tools() -> Vec<Tool> {
    vec![
        Tool::new(
            Cow::Borrowed(TOOL_GET_CROSS_SESSION_CALIBRATION),
            Cow::Borrowed(
                "The turn guard's cross-session calibration, whole: the settings in force, the \
                 rollup generation, every calibration row (candidate confidence bins and base \
                 rates), the thresholds in force, and the calibration block the guard would \
                 read.",
            ),
            object_schema(
                vec![
                    (
                        "as_of",
                        json!({"type": "string", "description": "RFC 3339 time to rebuild the snapshot at."}),
                    ),
                    (
                        "project_id",
                        json!({"type": "integer", "description": "Project whose block to render."}),
                    ),
                ],
                &[],
            ),
        ),
        Tool::new(
            Cow::Borrowed(TOOL_LIST_DIRECTIVE_PROPOSALS),
            Cow::Borrowed(
                "List directive proposals mike found from instructions the operator had to repeat \
                 and from hand-backs the operator confirmed were authorised, newest first, \
                 whole. Read-only. With `id`, returns that one proposal with every evidence item \
                 and the operator's words in full.",
            ),
            object_schema(
                vec![
                    (
                        "id",
                        json!({"type": "integer", "description": "One proposal's id."}),
                    ),
                    (
                        "status",
                        json!({"type": "string", "enum": ["open", "approved", "rejected", "superseded", "expired"]}),
                    ),
                    ("cursor", json!({"type": "integer"})),
                    (
                        "limit",
                        json!({"type": "integer", "description": "1 to 200, default 50."}),
                    ),
                ],
                &[],
            ),
        ),
    ]
}

fn pairs(
    arguments: Option<&Map<String, Value>>,
    keys: &[&'static str],
) -> Vec<(&'static str, String)> {
    let Some(a) = arguments else {
        return Vec::new();
    };
    keys.iter()
        .filter_map(|k| {
            let s = match a.get(*k)? {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                _ => return None,
            };
            Some((*k, s))
        })
        .collect()
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    let args = arguments.as_ref();
    let value = match name {
        TOOL_GET_CROSS_SESSION_CALIBRATION => {
            client
                .cross_session_get(
                    "/api/cross-session/calibration",
                    &pairs(args, &["as_of", "project_id"]),
                )
                .await
        }
        TOOL_LIST_DIRECTIVE_PROPOSALS => {
            match args.and_then(|a| a.get("id")).and_then(Value::as_i64) {
                Some(id) => {
                    client
                        .cross_session_get(&format!("/api/directive-proposals/{id}"), &[])
                        .await
                }
                None => {
                    client
                        .cross_session_get(
                            "/api/directive-proposals",
                            &pairs(args, &["status", "cursor", "limit"]),
                        )
                        .await
                }
            }
        }
        _ => {
            return Err(RmcpError::invalid_params(
                format!("unknown cross-session tool: {name}"),
                None,
            ));
        }
    };
    Ok(wrap_success(
        "cross_session",
        value.map_err(McpError::into_rmcp)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cross_session_tools_serve_every_name() {
        assert_eq!(cross_session_tools().len(), CROSS_SESSION_TOOL_NAMES.len());
        assert!(owns(TOOL_LIST_DIRECTIVE_PROPOSALS));
    }
}
