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

pub const TURNS_SUMMARY_TOOL_NAMES: &[&str] = &[
    TOOL_GET_SESSION_OVERVIEW,
    TOOL_LIST_CORRECTED_TURNS,
    TOOL_LIST_OUTCOME_SUMMARY,
];

pub const TOOL_GET_SESSION_OVERVIEW: &str = "get_session_overview";
pub const TOOL_LIST_CORRECTED_TURNS: &str = "list_corrected_turns";
pub const TOOL_LIST_OUTCOME_SUMMARY: &str = "list_outcome_summary";

pub fn turns_summary_tools() -> Vec<Tool> {
    vec![
        get_session_overview_tool(),
        list_corrected_turns_tool(),
        list_outcome_summary_tool(),
    ]
}

pub fn owns(name: &str) -> bool {
    TURNS_SUMMARY_TOOL_NAMES.contains(&name)
}

fn get_session_overview_tool() -> Tool {
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
                                   Absent = all turns (full-fidelity default)."
                }),
            ),
            (
                "limit",
                json!({
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 500,
                    "description": "Maximum turns to return. Absent = server cap (1000). \
                                   Returns total_turns + offset + limit in envelope."
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
            (
                "fields",
                json!({
                    "type": "string",
                    "description": "Projection mode. \"prompts\" omits stop_response for a lean \
                                   token-reduced view (user_prompt + prompt_kind remain). \
                                   Any other value (or absent) returns the full shape."
                }),
            ),
        ],
        &["session_id"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_GET_SESSION_OVERVIEW),
        Cow::Borrowed(
            "Per-turn digest of a session: operator prompt and full stop response for each \
             turn, with stop_reason, stop_request_id, interrupted flag, outcome, prompt_kind \
             and the serving model id. Returns {session_id, total_turns, offset, limit, turns}. \
             `fields=prompts` omits stop_response. Full turn detail: \
             `get_session_operator_turns`.",
        ),
        schema,
    )
}

fn list_corrected_turns_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "project",
                json!({
                    "type": "string",
                    "description": "Optional case-insensitive substring filter on project_path (matches `list_sessions`)."
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
                    "description": "RFC 3339 lower bound on created_at (inclusive). Absent → 7-day default window."
                }),
            ),
            (
                "date_to",
                json!({
                    "type": "string",
                    "description": "RFC 3339 upper bound on created_at (exclusive). Absent → now (paired with 7-day default)."
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
                    "description": "Maximum rows (default 50, max 500)."
                }),
            ),
            (
                "offset",
                json!({
                    "type": "integer",
                    "minimum": 0,
                    "description": "Skip this many rows. Default 0."
                }),
            ),
            ("order", order_property()),
        ],
        &[],
    );
    Tool::new(
        Cow::Borrowed(TOOL_LIST_CORRECTED_TURNS),
        Cow::Borrowed(
            "Turns the operator corrected, and the final turn of each session the \
             operator closed in frustration (`outcome` `abandoned`, `outcome_source` \
             `frustration_close`). Each row: full prompt, assistant text, \
             outcome_evidence and counts_as_correction.",
        ),
        schema,
    )
}

fn list_outcome_summary_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "project",
                json!({
                    "type": "string",
                    "description": "Optional exact-match filter on project_path."
                }),
            ),
            (
                "since",
                json!({
                    "type": "string",
                    "description": "Optional RFC 3339 lower bound on created_at."
                }),
            ),
        ],
        &[],
    );
    Tool::new(
        Cow::Borrowed(TOOL_LIST_OUTCOME_SUMMARY),
        Cow::Borrowed(
            "Counts of operator turns grouped by outcome category \
             (`unknown|accepted|corrected|abandoned|shipped`). Returns one \
             row per outcome that has at least one match, plus `total`, \
             `corrections` (turns that count as a correction) and \
             `frustration_closes`.",
        ),
        schema,
    )
}

#[derive(Debug, Deserialize)]
pub(crate) struct GetSessionOverviewArgs {
    pub(crate) session_id: String,
    #[serde(default)]
    pub(crate) order: Option<String>,
    #[serde(default)]
    pub(crate) prompt_kind: Option<String>,
    #[serde(default)]
    pub(crate) limit: Option<u64>,
    #[serde(default)]
    pub(crate) offset: Option<u64>,
    #[serde(default)]
    pub(crate) fields: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct ListCorrectedTurnsArgs {
    #[serde(default)]
    pub(crate) project: Option<String>,
    #[serde(default)]
    pub(crate) since: Option<String>,
    #[serde(default)]
    pub(crate) date_from: Option<String>,
    #[serde(default)]
    pub(crate) date_to: Option<String>,
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

#[derive(Debug, Deserialize, Default)]
pub(crate) struct ListOutcomeSummaryArgs {
    #[serde(default)]
    pub(crate) project: Option<String>,
    #[serde(default)]
    pub(crate) since: Option<String>,
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    match name {
        TOOL_GET_SESSION_OVERVIEW => {
            let args = parse_args_required::<GetSessionOverviewArgs>(arguments)?;
            let value = client
                .get_session_overview(
                    &args.session_id,
                    crate::client::SessionOverviewQuery {
                        order: args.order.as_deref(),
                        prompt_kind: args.prompt_kind.as_deref(),
                        limit: args.limit,
                        offset: args.offset,
                        fields: args.fields.as_deref(),
                    },
                )
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("session_overview", value))
        }
        TOOL_LIST_CORRECTED_TURNS => {
            let args = parse_args_default::<ListCorrectedTurnsArgs>(arguments)?;
            let mut q: Vec<(&str, String)> = Vec::new();
            q.push(("outcome", "correction".to_string()));
            if let Some(ref p) = args.project {
                q.push(("project", p.clone()));
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
            let (value, note) = client
                .get_json_with_note("/api/operator-turns/by-outcome", &q)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success_with_note("corrected_turns", value, note))
        }
        TOOL_LIST_OUTCOME_SUMMARY => {
            let args = parse_args_default::<ListOutcomeSummaryArgs>(arguments)?;
            let value = client
                .list_outcome_summary(args.project.as_deref(), args.since.as_deref())
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("outcome_summary", value))
        }
        other => Err(RmcpError::invalid_params(
            format!("unknown operator turn summary tool: {other}"),
            None,
        )),
    }
}
