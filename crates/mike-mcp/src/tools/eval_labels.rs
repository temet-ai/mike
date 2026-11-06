use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde_json::{Map, Value, json};

use super::{object_schema, wrap_success};
use crate::client::InspectClient;
use crate::error::McpError;

pub const TOOL_RELAY_LABEL: &str = "relay_label";
pub const TOOL_LIST_EVAL_LABELS: &str = "list_eval_labels";

pub const EVAL_LABEL_TOOL_NAMES: &[&str] = &[TOOL_RELAY_LABEL, TOOL_LIST_EVAL_LABELS];

pub fn eval_label_tools() -> Vec<Tool> {
    vec![relay_label_tool(), list_eval_labels_tool()]
}

pub fn owns(name: &str) -> bool {
    EVAL_LABEL_TOOL_NAMES.contains(&name)
}

fn text(description: &str) -> Value {
    json!({"type": "string", "description": description})
}

fn relay_label_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "subject_kind",
                json!({"type": "string", "enum": ["turn", "session_end"]}),
            ),
            (
                "turn_id",
                json!({"type": "integer", "description": "The turn, for subject_kind turn."}),
            ),
            (
                "session_id",
                text("Session UUID, for subject_kind session_end."),
            ),
            (
                "axis",
                json!({"type": "string", "enum": ["reaction", "delivery", "session_end"]}),
            ),
            (
                "label",
                json!({"type": "string", "enum": [
                "corrected", "redirected", "accepted", "no_response", "unclear",
                "shipped", "delivered", "partial", "none", "question", "not_applicable",
                "done", "closed_in_frustration", "cut_off_neutral", "fleet_seat"]}),
            ),
            (
                "operator_quote",
                text(
                    "The operator's exact words, at least 8 characters, copied from their message.",
                ),
            ),
            (
                "quote_session_id",
                text("Session UUID the words were said in; default your own session."),
            ),
            ("note", text("Optional note.")),
            (
                "relayed_by_session_id",
                text("Your session UUID; defaults to CLAUDE_CODE_SESSION_ID."),
            ),
        ],
        &["subject_kind", "axis", "label", "operator_quote"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_RELAY_LABEL),
        Cow::Borrowed(
            "Record, as a relayed label, what the operator told you about how a turn was received \
             or how a session ended. Relayed labels are never gold; gold labels come only from \
             the operator's clicks in the mike UI. Use it when the operator says something about \
             a specific turn or session, such as \"that turn wasn't abandoned, it was done\" or \
             \"you got that wrong\". The call is refused unless `operator_quote` appears in a \
             captured operator prompt of `quote_session_id` (default: your own session). Axes: \
             `reaction` (corrected, redirected, accepted, no_response, unclear) and `delivery` \
             (shipped, delivered, partial, none, question, not_applicable) label a turn; \
             `session_end` (done, closed_in_frustration, cut_off_neutral, fleet_seat, unclear) \
             labels a session, and mike applies it to the session's last closed turn.",
        ),
        schema,
    )
}

fn list_eval_labels_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "subject_kind",
                text("turn, session_end, evaluation or intervention."),
            ),
            ("turn_id", json!({"type": "integer"})),
            ("session", text("Session UUID.")),
            ("axis", text("Label axis.")),
            (
                "tier",
                text("Comma list of gold, relayed, auto_shadow, seed."),
            ),
            ("method", text("Label method.")),
            ("split", text("dev or holdout.")),
            ("source", text("Label source.")),
            ("current_only", json!({"type": "boolean"})),
            ("since", text("RFC 3339 lower bound.")),
            ("until", text("RFC 3339 upper bound.")),
            ("before_id", json!({"type": "integer"})),
            (
                "limit",
                json!({"type": "integer", "description": "1 to 500, default 100."}),
            ),
        ],
        &[],
    );
    Tool::new(
        Cow::Borrowed(TOOL_LIST_EVAL_LABELS),
        Cow::Borrowed(
            "List the labels mike holds for turns and session ends: gold (the operator's clicks), \
             relayed (an agent quoting the operator), auto_shadow and seed (the fleet-mirror \
             import), each with its split, source, full evidence and provenance, and mike's own \
             verdict at the time. Page with before_id.",
        ),
        schema,
    )
}

fn with_relayer(arguments: Option<Map<String, Value>>) -> Value {
    let mut map = arguments.unwrap_or_default();
    if !map.contains_key("relayed_by_session_id")
        && let Ok(session) = std::env::var("CLAUDE_CODE_SESSION_ID")
    {
        map.insert("relayed_by_session_id".to_owned(), Value::String(session));
    }
    Value::Object(map)
}

fn query_pairs(arguments: &Map<String, Value>) -> Vec<(&'static str, String)> {
    const KEYS: [&str; 13] = [
        "subject_kind",
        "turn_id",
        "session",
        "axis",
        "tier",
        "method",
        "split",
        "source",
        "current_only",
        "since",
        "until",
        "before_id",
        "limit",
    ];
    KEYS.iter()
        .filter_map(|key| {
            arguments.get(*key).map(|v| {
                (
                    *key,
                    v.as_str().map_or_else(|| v.to_string(), str::to_owned),
                )
            })
        })
        .collect()
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    match name {
        TOOL_RELAY_LABEL => {
            let body = with_relayer(arguments);
            let value = client
                .relay_label(&body)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("relay", value))
        }
        TOOL_LIST_EVAL_LABELS => {
            let pairs = query_pairs(&arguments.unwrap_or_default());
            let value = client
                .list_eval_labels(&pairs)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("labels", value))
        }
        other => Err(RmcpError::invalid_params(
            format!("unknown eval label tool: {other}"),
            None,
        )),
    }
}
