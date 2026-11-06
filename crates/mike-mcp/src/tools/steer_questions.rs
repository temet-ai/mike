use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde_json::{Map, Value, json};

use super::{object_schema, wrap_success};
use crate::client::InspectClient;
use crate::error::McpError;

pub const TOOL_GET_STEER_QUESTIONS: &str = "get_steer_questions";
pub const TOOL_LIST_STEER_QUESTION_EVENTS: &str = "list_steer_question_events";

pub const STEER_QUESTION_TOOL_NAMES: &[&str] =
    &[TOOL_GET_STEER_QUESTIONS, TOOL_LIST_STEER_QUESTION_EVENTS];

pub fn owns(name: &str) -> bool {
    STEER_QUESTION_TOOL_NAMES.contains(&name)
}

pub fn steer_question_tools() -> Vec<Tool> {
    vec![
        Tool::new(
            Cow::Borrowed(TOOL_GET_STEER_QUESTIONS),
            Cow::Borrowed(
                "List every turn-guard question, whole. Read-only. Each: its policy (`mode`, \
                 `holdback`), bar, promotion gate with counts and Wilson bounds, live arm counts, \
                 reach at due per arm, wake states, the holdback effect estimate, the verifier \
                 comparison for `unobserved_done` and the full promotion history.",
            ),
            object_schema(vec![], &[]),
        ),
        Tool::new(
            Cow::Borrowed(TOOL_LIST_STEER_QUESTION_EVENTS),
            Cow::Borrowed(
                "List the evaluation rows behind a guard question's numbers, newest first, whole: \
                 arm, draw, anchor, due time, reach at due, verdict JSON, labels, the scored turn, \
                 and for treated rows the intervention (payload, channel, wake state, delivery). \
                 Paginated by `cursor`.",
            ),
            object_schema(
                vec![
                    (
                        "steerer_id",
                        json!({"type": "integer", "description": "The turn-guard steerer id."}),
                    ),
                    (
                        "question",
                        json!({"type": "string", "enum": ["relapse", "unbacked_wait", "authorised_handback", "unobserved_done"]}),
                    ),
                    (
                        "arm",
                        json!({"type": "string", "enum": ["treated", "holdback"]}),
                    ),
                    (
                        "mode",
                        json!({"type": "string", "enum": ["live", "shadow"]}),
                    ),
                    (
                        "cursor",
                        json!({"type": "integer", "description": "next_cursor from the previous page."}),
                    ),
                    (
                        "limit",
                        json!({"type": "integer", "description": "Rows per page, 1 to 200, default 50."}),
                    ),
                ],
                &["steerer_id", "question"],
            ),
        ),
    ]
}

fn pairs(arguments: Option<&Map<String, Value>>) -> Vec<(&'static str, String)> {
    let Some(a) = arguments else {
        return Vec::new();
    };
    ["arm", "mode", "cursor", "limit"]
        .iter()
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
        TOOL_GET_STEER_QUESTIONS => {
            client
                .steer_questions_get("/api/steering/questions", &[])
                .await
        }
        TOOL_LIST_STEER_QUESTION_EVENTS => {
            let steerer = args
                .and_then(|a| a.get("steerer_id"))
                .and_then(Value::as_i64)
                .ok_or_else(|| {
                    RmcpError::invalid_params("steerer_id (integer) is required", None)
                })?;
            let question = args
                .and_then(|a| a.get("question"))
                .and_then(Value::as_str)
                .ok_or_else(|| RmcpError::invalid_params("question (string) is required", None))?;
            client
                .steer_questions_get(
                    &format!("/api/steering/questions/{steerer}/{question}/events"),
                    &pairs(args),
                )
                .await
        }
        _ => {
            return Err(RmcpError::invalid_params(
                format!("unknown steer question tool: {name}"),
                None,
            ));
        }
    };
    Ok(wrap_success(
        "steer_questions",
        value.map_err(McpError::into_rmcp)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steer_question_tools_serve_every_name() {
        assert_eq!(
            steer_question_tools().len(),
            STEER_QUESTION_TOOL_NAMES.len()
        );
        assert!(owns(TOOL_LIST_STEER_QUESTION_EVENTS));
    }
}
