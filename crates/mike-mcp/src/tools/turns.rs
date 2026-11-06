use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{
    object_schema, order_property, parse_args_default, parse_args_required, wrap_success,
    wrap_success_with_note,
};
use crate::client::InspectClient;
use crate::error::McpError;

pub const TURNS_TOOL_NAMES: &[&str] = &[
    TOOL_LIST_OPERATOR_TURNS,
    TOOL_GET_OPERATOR_TURN,
    TOOL_GET_SESSION_OPERATOR_TURNS,
];

pub const TOOL_LIST_OPERATOR_TURNS: &str = "list_operator_turns";
pub const TOOL_GET_OPERATOR_TURN: &str = "get_operator_turn";
pub const TOOL_GET_SESSION_OPERATOR_TURNS: &str = "get_session_operator_turns";

pub fn turns_tools() -> Vec<Tool> {
    vec![
        list_operator_turns_tool(),
        get_operator_turn_tool(),
        get_session_operator_turns_tool(),
    ]
}

pub fn owns(name: &str) -> bool {
    TURNS_TOOL_NAMES.contains(&name)
}

fn list_operator_turns_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "project",
                json!({
                    "type": "string",
                    "description": "Optional case-insensitive substring filter on `projects.projects.project_path` (matches `list_sessions`)."
                }),
            ),
            (
                "session",
                json!({
                    "type": "string",
                    "description": "Optional filter on session id."
                }),
            ),
            (
                "since",
                json!({
                    "type": "string",
                    "description": "RFC 3339 lower bound. Alias for `date_from`; `date_from` wins if both given."
                }),
            ),
            (
                "date_from",
                json!({
                    "type": "string",
                    "description": "RFC 3339 lower bound on `operator_turns.created_at` (inclusive). Absent → 7-day default window."
                }),
            ),
            (
                "date_to",
                json!({
                    "type": "string",
                    "description": "RFC 3339 upper bound on `operator_turns.created_at` (exclusive). Absent → now (paired with 7-day default)."
                }),
            ),
            (
                "outcome",
                json!({
                    "type": "string",
                    "enum": ["unknown", "accepted", "corrected", "abandoned", "shipped", "correction"],
                    "description": "Optional outcome filter; the five codes match the DB CHECK constraint, and `correction` selects every turn that counts as a correction (corrected, or abandoned by a frustration close)."
                }),
            ),
            (
                "subject_command",
                json!({
                    "type": "string",
                    "description": "Optional exact-match filter on the slash-command/skill that opened the turn (e.g. \"/implement\")."
                }),
            ),
            (
                "subject_agent",
                json!({
                    "type": "string",
                    "description": "Optional exact-match filter on the agent persona active for the turn (e.g. \"rust-developer\"; \"main\" for direct operator turns)."
                }),
            ),
            (
                "skill",
                json!({
                    "type": "string",
                    "description": "Optional filter: only turns whose skills_used includes this skill name (agent-invoked skills mid-turn via the Skill tool)."
                }),
            ),
            (
                "limit",
                json!({
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 500,
                    "description": "Maximum rows per page (default 50, max 500)."
                }),
            ),
            (
                "offset",
                json!({
                    "type": "integer",
                    "minimum": 0,
                    "description": "Skip this many rows before returning. Default 0."
                }),
            ),
            ("order", order_property()),
        ],
        &[],
    );
    Tool::new(
        Cow::Borrowed(TOOL_LIST_OPERATOR_TURNS),
        Cow::Borrowed(
            "Paginated list of operator turns (one per user prompt), newest-first. \
             Each row: full user_prompt, final_assistant_text, outcome_evidence, \
             aggregate counts (tool calls, errors, files, tokens, outcome label), \
             and the serving model id for the turn.",
        ),
        schema,
    )
}

fn get_operator_turn_tool() -> Tool {
    let schema = object_schema(
        vec![(
            "id",
            json!({
                "type": "integer",
                "minimum": 1,
                "description": "Operator turn id (BIGSERIAL primary key on operator_turns)."
            }),
        )],
        &["id"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_GET_OPERATOR_TURN),
        Cow::Borrowed(
            "Fetch a single operator turn by `id`. Returns the full prompt, full assistant \
             text, full outcome evidence and all aggregate counts. 404 when the id does not \
             exist; 503 when persistence is disabled.",
        ),
        schema,
    )
}

fn get_session_operator_turns_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "session_id",
                json!({
                    "type": "string",
                    "description": "Session id (a canonical UUID from list_sessions)."
                }),
            ),
            ("order", order_property()),
            (
                "prompt_kind",
                json!({
                    "type": "string",
                    "description": "Optional filter: restrict to turns of this kind \
                                   (operator|activity-label|hook-feedback|teammate-message| \
                                   session-recap|background-event|compaction|analysis-probe). \
                                   When absent, all turns are returned."
                }),
            ),
            (
                "limit",
                json!({
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 500,
                    "description": "Maximum turns to return (default: server cap of 1000)."
                }),
            ),
            (
                "offset",
                json!({
                    "type": "integer",
                    "minimum": 0,
                    "description": "Skip this many turns (0-based). Default 0."
                }),
            ),
        ],
        &["session_id"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_GET_SESSION_OPERATOR_TURNS),
        Cow::Borrowed(
            "All operator turns for a session, each whole. Each turn carries prompt_kind \
             (operator|activity-label|hook-feedback|teammate-message|session-recap|\
             background-event|compaction|analysis-probe) and the serving model id for the turn \
             (the model of the turn's opening request; absent when that request was \
             partition-evicted).",
        ),
        schema,
    )
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct ListOperatorTurnsArgs {
    #[serde(default)]
    pub(crate) project: Option<String>,
    #[serde(default)]
    pub(crate) session: Option<String>,
    #[serde(default)]
    pub(crate) since: Option<String>,
    #[serde(default)]
    pub(crate) date_from: Option<String>,
    #[serde(default)]
    pub(crate) date_to: Option<String>,
    #[serde(default)]
    pub(crate) outcome: Option<String>,
    #[serde(default)]
    pub(crate) subject_command: Option<String>,
    #[serde(default)]
    pub(crate) subject_agent: Option<String>,
    #[serde(default)]
    pub(crate) skill: Option<String>,
    #[serde(default)]
    pub(crate) limit: Option<u64>,
    #[serde(default)]
    pub(crate) offset: Option<u64>,
    #[serde(default)]
    pub(crate) order: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GetOperatorTurnArgs {
    pub(crate) id: i64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GetSessionOperatorTurnsArgs {
    pub(crate) session_id: String,
    #[serde(default)]
    pub(crate) order: Option<String>,
    #[serde(default)]
    pub(crate) prompt_kind: Option<String>,
    #[serde(default)]
    pub(crate) limit: Option<u64>,
    #[serde(default)]
    pub(crate) offset: Option<u64>,
}

fn list_operator_turns_query(args: &ListOperatorTurnsArgs) -> Vec<(&'static str, String)> {
    let mut q: Vec<(&'static str, String)> = Vec::new();
    if let Some(ref p) = args.project {
        q.push(("project", p.clone()));
    }
    if let Some(ref s) = args.session {
        q.push(("session", s.clone()));
    }
    if let Some(ref s) = args.since {
        q.push(("since", s.clone()));
    }
    if let Some(ref df) = args.date_from {
        q.push(("date_from", df.clone()));
    }
    if let Some(ref dt) = args.date_to {
        q.push(("date_to", dt.clone()));
    }
    if let Some(ref o) = args.outcome {
        q.push(("outcome", o.clone()));
    }
    if let Some(ref sc) = args.subject_command {
        q.push(("subject_command", sc.clone()));
    }
    if let Some(ref sa) = args.subject_agent {
        q.push(("subject_agent", sa.clone()));
    }
    if let Some(ref sk) = args.skill {
        q.push(("skill", sk.clone()));
    }
    if let Some(l) = args.limit {
        q.push(("limit", l.to_string()));
    }
    if let Some(o) = args.offset {
        q.push(("offset", o.to_string()));
    }
    if let Some(ref ord) = args.order {
        q.push(("order", ord.clone()));
    }
    q
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    match name {
        TOOL_LIST_OPERATOR_TURNS => {
            let args = parse_args_default::<ListOperatorTurnsArgs>(arguments)?;
            let q = list_operator_turns_query(&args);
            let (value, note) = client
                .get_json_with_note("/api/operator-turns", &q)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success_with_note("operator_turns", value, note))
        }
        TOOL_GET_OPERATOR_TURN => {
            let args = parse_args_required::<GetOperatorTurnArgs>(arguments)?;
            let value = client
                .get_operator_turn(args.id)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("operator_turn", value))
        }
        TOOL_GET_SESSION_OPERATOR_TURNS => {
            let args = parse_args_required::<GetSessionOperatorTurnsArgs>(arguments)?;
            let value = client
                .get_session_operator_turns(
                    &args.session_id,
                    args.order.as_deref(),
                    args.prompt_kind.as_deref(),
                    args.limit,
                    args.offset,
                )
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("operator_turns", value))
        }
        other => Err(RmcpError::invalid_params(
            format!("unknown operator turn tool: {other}"),
            None,
        )),
    }
}
