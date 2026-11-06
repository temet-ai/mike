use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde_json::{Map, Value, json};

use super::{object_schema, wrap_success};
use crate::client::InspectClient;
use crate::error::McpError;

pub const TOOL_GET_SESSION_VERDICTS: &str = "get_session_verdicts";
pub const TOOL_LIST_EVAL_SESSION_END_ITEMS: &str = "list_eval_session_end_items";

pub const SESSION_VERDICT_TOOL_NAMES: &[&str] =
    &[TOOL_GET_SESSION_VERDICTS, TOOL_LIST_EVAL_SESSION_END_ITEMS];

pub fn owns(name: &str) -> bool {
    SESSION_VERDICT_TOOL_NAMES.contains(&name)
}

fn integer(description: &str) -> Value {
    json!({"type": "integer", "description": description})
}

pub fn session_verdict_tools() -> Vec<Tool> {
    vec![
        Tool::new(
            Cow::Borrowed(TOOL_GET_SESSION_VERDICTS),
            Cow::Borrowed(
                "Every session-judge verdict row of one session, newest first, whole: outcome, \
                 score, rationale, mode (live, shadow or eval), bundle, the end label with its \
                 source, rule, confidence and evidence, failure tags, severity, the judge input \
                 and output. `end` is the session's effective end label, where an operator gold \
                 label outranks the judge.",
            ),
            object_schema(
                vec![
                    (
                        "session_id",
                        json!({"type": "string", "description": "Session UUID."}),
                    ),
                    (
                        "mode",
                        json!({"type": "string", "enum": ["live", "shadow", "eval"], "description": "Only rows of this mode; omit for every mode."}),
                    ),
                    ("limit", integer("Rows per page, 1 to 500, default 50.")),
                    ("offset", integer("Rows to skip.")),
                ],
                &["session_id"],
            ),
        ),
        Tool::new(
            Cow::Borrowed(TOOL_LIST_EVAL_SESSION_END_ITEMS),
            Cow::Borrowed(
                "The session-end items of one `mike eval run --role session_judge`: the gold end \
                 label, the model's end label, the deterministic rule that held, the effective \
                 label and whether frustration was called correctly.",
            ),
            object_schema(
                vec![
                    ("run_id", integer("Eval run id.")),
                    ("limit", integer("Rows per page, 1 to 1000, default 100.")),
                    ("offset", integer("Rows to skip.")),
                ],
                &["run_id"],
            ),
        ),
    ]
}

fn paging(
    arguments: Option<&Map<String, Value>>,
    keys: &[&'static str],
) -> Vec<(&'static str, String)> {
    let Some(arguments) = arguments else {
        return Vec::new();
    };
    keys.iter()
        .filter_map(|k| {
            let v = arguments.get(*k)?;
            let s = match v {
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
        TOOL_GET_SESSION_VERDICTS => {
            let session = args
                .and_then(|a| a.get("session_id"))
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    RmcpError::invalid_params("session_id (string) is required", None)
                })?;
            client
                .session_verdicts_get(
                    &format!("/api/sessions/{session}/verdicts"),
                    &paging(args, &["mode", "limit", "offset"]),
                )
                .await
        }
        TOOL_LIST_EVAL_SESSION_END_ITEMS => {
            let id = args
                .and_then(|a| a.get("run_id"))
                .and_then(Value::as_i64)
                .ok_or_else(|| RmcpError::invalid_params("run_id (integer) is required", None))?;
            client
                .session_verdicts_get(
                    &format!("/api/eval/runs/{id}/session-end-items"),
                    &paging(args, &["limit", "offset"]),
                )
                .await
        }
        _ => {
            return Err(RmcpError::invalid_params(
                format!("unknown session verdict tool: {name}"),
                None,
            ));
        }
    };
    Ok(wrap_success(
        "session_verdicts",
        value.map_err(McpError::into_rmcp)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_verdict_tools_serve_every_name() {
        let tools = session_verdict_tools();
        assert_eq!(tools.len(), SESSION_VERDICT_TOOL_NAMES.len());
        assert!(owns(TOOL_GET_SESSION_VERDICTS));
    }
}
