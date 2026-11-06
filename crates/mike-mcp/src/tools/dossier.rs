use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{object_schema, parse_args_required, wrap_success};
use crate::client::InspectClient;
use crate::error::McpError;

pub const TOOL_GET_SESSION_DOSSIER: &str = "get_session_dossier";
pub const TOOL_GET_TURN_FACTS: &str = "get_turn_facts";

pub const DOSSIER_TOOL_NAMES: &[&str] = &[TOOL_GET_SESSION_DOSSIER, TOOL_GET_TURN_FACTS];

pub fn dossier_tools() -> Vec<Tool> {
    vec![get_session_dossier_tool(), get_turn_facts_tool()]
}

pub fn owns(name: &str) -> bool {
    DOSSIER_TOOL_NAMES.contains(&name)
}

fn text(description: &str) -> Value {
    json!({"type": "string", "description": description})
}

fn integer(description: &str) -> Value {
    json!({"type": "integer", "description": description})
}

fn get_session_dossier_tool() -> Tool {
    let schema = object_schema(
        vec![
            ("session_id", text("Session UUID.")),
            ("as_of", text("RFC 3339 moment; default now.")),
            (
                "as_of_turn",
                integer("Use this turn's closed_at as the moment."),
            ),
            (
                "mode",
                text(
                    "knowledge (what mike knew then, default) or reconstructed (evaluation over history).",
                ),
            ),
            ("audience", text("none, judge, session_judge or steerer.")),
            ("turn_id", integer("The subject turn for judge or steerer.")),
            (
                "slices",
                text("session_judge slices: reactions,promises,directives."),
            ),
            (
                "sections",
                text(
                    "steerer sections: ending,reactions,directives,promises,asks,corrections,notes,opening,turns.",
                ),
            ),
            (
                "budget_chars",
                integer("Character budget for session_judge and steerer; default 24000."),
            ),
            ("section", text("Limit the view to one section.")),
            ("limit", integer("Items per section page; default 500.")),
            ("offset", integer("Item offset per section.")),
        ],
        &["session_id"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_GET_SESSION_DOSSIER),
        Cow::Borrowed(
            "Everything mike holds about a session as of a moment, whole: the prompt stream with \
             each message's capture and effective kind, the turns with their close-fixed facts, \
             the operator's asks, directives and promises, prior corrections and live reactions. \
             `view` is what mike holds; `rendered` is the exact text a judge, session judge or \
             steerer model would receive for the chosen audience and budget.",
        ),
        schema,
    )
}

fn get_turn_facts_tool() -> Tool {
    let schema = object_schema(
        vec![("turn_id", integer("Operator turn id."))],
        &["turn_id"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_GET_TURN_FACTS),
        Cow::Borrowed(
            "Every memory.turn_facts column of one operator turn, whole, with the next genuine \
             message's text, the turn's opened_by, and the resolver's answer as of now beside the \
             stored hindsight columns.",
        ),
        schema,
    )
}

#[derive(Debug, Deserialize)]
struct DossierArgs {
    session_id: String,
    #[serde(flatten)]
    query: Map<String, Value>,
}

#[derive(Debug, Deserialize)]
struct TurnFactsArgs {
    turn_id: i64,
}

fn query_pairs(query: &Map<String, Value>) -> Vec<(&'static str, String)> {
    const KEYS: [&str; 11] = [
        "as_of",
        "as_of_turn",
        "mode",
        "audience",
        "turn_id",
        "slices",
        "sections",
        "budget_chars",
        "section",
        "limit",
        "offset",
    ];
    KEYS.iter()
        .filter_map(|key| {
            query.get(*key).map(|value| {
                let text = value
                    .as_str()
                    .map_or_else(|| value.to_string(), str::to_owned);
                (*key, text)
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
        TOOL_GET_SESSION_DOSSIER => {
            let args = parse_args_required::<DossierArgs>(arguments)?;
            let value = client
                .get_session_dossier(&args.session_id, &query_pairs(&args.query))
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("dossier", value))
        }
        TOOL_GET_TURN_FACTS => {
            let args = parse_args_required::<TurnFactsArgs>(arguments)?;
            let value = client
                .get_turn_facts(args.turn_id)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("turn_facts", value))
        }
        other => Err(RmcpError::invalid_params(
            format!("unknown dossier tool: {other}"),
            None,
        )),
    }
}
