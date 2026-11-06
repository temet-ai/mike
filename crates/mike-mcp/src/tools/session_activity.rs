use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{object_schema, order_property, parse_args_default, parse_args_required, wrap_success};
use crate::client::InspectClient;
use crate::error::McpError;

pub const SESSION_ACTIVITY_TOOL_NAMES: &[&str] = &[
    TOOL_LIST_SESSION_TOOL_USES,
    TOOL_CONFIRM_MESSAGE_RECEIPT,
    TOOL_LIST_FILE_WRITES,
    TOOL_SESSION_TOOL_USE_STATS,
    TOOL_SET_SESSION_GOAL,
];

pub const TOOL_LIST_SESSION_TOOL_USES: &str = "list_session_tool_uses";
pub const TOOL_CONFIRM_MESSAGE_RECEIPT: &str = "confirm_message_receipt";
pub const TOOL_LIST_FILE_WRITES: &str = "list_file_writes";
pub const TOOL_SESSION_TOOL_USE_STATS: &str = "session_tool_use_stats";
pub const TOOL_SET_SESSION_GOAL: &str = "set_session_goal";

pub fn session_activity_tools() -> Vec<Tool> {
    vec![
        list_session_tool_uses_tool(),
        confirm_message_receipt_tool(),
        list_file_writes_tool(),
        session_tool_use_stats_tool(),
        set_session_goal_tool(),
    ]
}

pub fn owns(name: &str) -> bool {
    SESSION_ACTIVITY_TOOL_NAMES.contains(&name)
}

fn list_session_tool_uses_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "id",
                json!({
                    "type": "string",
                    "description": "Session id (a canonical UUID from /api/sessions)."
                }),
            ),
            (
                "offset",
                json!({
                    "type": "integer",
                    "minimum": 0,
                    "description": "Skip this many tool-use rows — counted in the selected `order` direction (default desc = newest first). Default 0."
                }),
            ),
            (
                "limit",
                json!({
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 1000,
                    "description": "Maximum number of tool-use rows to return (default 200, max 1000). Each row carries the FULL captured `input` verbatim."
                }),
            ),
            ("order", order_property()),
            (
                "include_results",
                json!({
                    "type": "boolean",
                    "description": "Default true (full fidelity). Pass false to omit each row's full `result` content while keeping `result_bytes`/`is_error` — use for a size/error scan over a large session."
                }),
            ),
            (
                "agent",
                json!({
                    "type": "string",
                    "description": "Scope to one subagent: a registered agent name (`rust-developer`) or a persona string (`Rust Development Expert`). Omit for every call in the session, the parent's own included. `agent_name` and `subagent_persona` are not interchangeable; copy the value from an unfiltered page."
                }),
            ),
        ],
        &["id"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_LIST_SESSION_TOOL_USES),
        Cow::Borrowed(
            "List every tool call captured for session `id`. Each row: tool_name, tool_id, request_id, \
             full input, file_path for Read/Write/Edit calls, result_bytes, is_error and, unless \
             include_results is false, the full paired result. `agent_name` and `agent_instance_id` \
             identify a dispatched subagent; `subagent_persona` marks subagent traffic with no dispatch \
             bound to it (a spawned `--agent` session); both absent means the session's own primary lane. \
             Pass `agent` to scope to one worker. Returns {session_id, total_count, offset, limit, rows}.",
        ),
        schema,
    )
}

fn confirm_message_receipt_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "session_id",
                json!({"type": "string", "description": "The ADDRESSED session's UUID — the recipient, not the sender."}),
            ),
            (
                "message_uuid",
                json!({"type": "string", "description": "The mailbox message uuid to confirm."}),
            ),
        ],
        &["session_id", "message_uuid"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_CONFIRM_MESSAGE_RECEIPT),
        Cow::Borrowed(
            "Check whether the addressed session acted on a mailbox message. Searches the recipient's own \
             captured traffic after the message was queued for a citation of the message uuid or a \
             distinctive run of its body. Returns {confidence, delivery_row, receipt_evidence[], caveat}; \
             confidence, strongest first: acknowledged (it acked), high (its traffic cites the uuid), \
             medium (its traffic quotes the body), none. Call this before asserting a session never got a \
             message: a delivery row is not receipt, because a session that pulls its own inbox acts on a \
             message while claimed_at and consumed_at stay null. confidence=none means no citation was \
             found, not that the message did not arrive. verify_message_read is the sender-scoped tool that \
             reports the delivery row itself. Served only under the `full` profile: a session \
             outside a fleet that must settle whether a peer acted on a message calls it \
             (MIKE_MCP_PROFILE=full); a fleet seat asks the peer.",
        ),
        schema,
    )
}

fn list_file_writes_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "project",
                json!({
                    "type": "string",
                    "description": "Substring match against the project path (same shape as list_sessions). Omit to span every project."
                }),
            ),
            (
                "from",
                json!({
                    "type": "string",
                    "description": "RFC 3339 lower bound on write time, inclusive. A malformed value is a 400, never a silently widened window."
                }),
            ),
            (
                "to",
                json!({
                    "type": "string",
                    "description": "RFC 3339 upper bound on write time, exclusive."
                }),
            ),
            (
                "path",
                json!({
                    "type": "string",
                    "description": "Substring match against the written file path."
                }),
            ),
            (
                "collisions_only",
                json!({
                    "type": "boolean",
                    "description": "Keep only files written by 2+ distinct SESSIONS. Two dispatches inside one session are that session's own business and do not count."
                }),
            ),
            (
                "offset",
                json!({"type": "integer", "minimum": 0, "description": "Files to skip. Default 0."}),
            ),
            (
                "limit",
                json!({"type": "integer", "minimum": 1, "maximum": 500, "description": "Files per page (default 100, max 500)."}),
            ),
        ],
        &[],
    );
    Tool::new(
        Cow::Borrowed(TOOL_LIST_FILE_WRITES),
        Cow::Borrowed(
            "List which sessions wrote to which files and where two sessions collided on one. Returns files \
             most-contended first, each with session_count, write_count, net_line_delta and every write \
             oldest first: {session_id, request_id, tool_name, created_at, agent_name, subagent_persona, \
             line_delta}. write_count counts editing-tool calls, not changes: a file edited and then \
             reverted shows writes and is byte-identical. net_line_delta is lines added minus removed: \
             non-zero proves the file changed; zero does not prove it did not, since a revert or any \
             same-line-count substitution nets to zero, so diff before concluding. `collisions_only: true` \
             keeps only files that 2+ sessions wrote. Covers Edit, Write and NotebookEdit only: Bash never \
             records a file path, so git operations, shell redirects and stashes are invisible, and an \
             empty result means no captured file-tool write, not that nothing touched the file.",
        ),
        schema,
    )
}

fn session_tool_use_stats_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "id",
                json!({
                    "type": "string",
                    "description": "Session id (a canonical UUID from /api/sessions)."
                }),
            ),
            (
                "top_n",
                json!({
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 100,
                    "description": "Number of largest-result rows to return (default 20, max 100)."
                }),
            ),
        ],
        &["id"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_SESSION_TOOL_USE_STATS),
        Cow::Borrowed(
            "Tool-call efficiency aggregates for session `id`: per-tool-name counts and total result bytes; \
             duplicate-input groups (same tool_name and input; wasted bytes = (count-1) * avg result bytes; \
             sample request ids; input excerpt); and the top-N largest results (tool_name, request_id, \
             wire tool id, result_bytes, input excerpt). Use it to find where a session's context went.",
        ),
        schema,
    )
}

fn set_session_goal_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "session_id",
                json!({
                    "type": "string",
                    "description": "Session id (a canonical UUID from list_sessions). \
                                    An agent should pass its own session id here."
                }),
            ),
            (
                "name",
                json!({
                    "type": "string",
                    "description": "The name peers see for this session on the roster and in \
                                    briefings (e.g. \"observer-1\", \"rust-dev-alpha\"). Not \
                                    unique — two live sessions may share a name."
                }),
            ),
            (
                "role",
                json!({
                    "type": "string",
                    "description": "The role this session answers to. Free text, open set. \
                                    This is exactly the value a `role:<role>` mailbox send \
                                    fans out to — declare it here to be reachable by role."
                }),
            ),
            (
                "instructions",
                json!({
                    "type": "string",
                    "description": "The standing brief this session works under — distinct \
                                    from `goal` (what it is doing now) and `next_steps` (what \
                                    it does next)."
                }),
            ),
            (
                "goal",
                json!({
                    "type": "string",
                    "description": "Free-text goal for this session — what the session is trying to accomplish."
                }),
            ),
            (
                "next_steps",
                json!({
                    "type": "string",
                    "description": "The concrete next steps the session plans to take."
                }),
            ),
            (
                "set_by",
                json!({
                    "type": "string",
                    "description": "Who set the goal. Defaults to \"mcp\" when omitted."
                }),
            ),
        ],
        &["session_id"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_SET_SESSION_GOAL),
        Cow::Borrowed(
            "Declare this session's identity in one call: the `name` peers see, the `role` it answers to \
             for `role:` mailbox fan-out (send_session_message), the standing `instructions` it works \
             under, and the `goal` and `next_steps` it is on now. Only `session_id` is required; an \
             omitted field keeps its value, so setting just `name` does not clear `goal`. \
             register_workspace_session sets the same fields and also joins the roster. Peers read the \
             fields through get_workspace_roster; steerers read them too.",
        ),
        schema,
    )
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListSessionToolUsesArgs {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) offset: Option<u64>,
    #[serde(default)]
    pub(crate) limit: Option<u64>,
    #[serde(default)]
    pub(crate) order: Option<String>,
    #[serde(default)]
    pub(crate) include_results: Option<bool>,
    #[serde(default)]
    pub(crate) agent: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct ConfirmMessageReceiptArgs {
    pub(crate) session_id: String,
    pub(crate) message_uuid: String,
}

#[derive(Debug, Default, serde::Deserialize)]
pub(crate) struct ListFileWritesArgs {
    #[serde(default)]
    pub(crate) project: Option<String>,
    #[serde(default)]
    pub(crate) from: Option<String>,
    #[serde(default)]
    pub(crate) to: Option<String>,
    #[serde(default)]
    pub(crate) path: Option<String>,
    #[serde(default)]
    pub(crate) collisions_only: Option<bool>,
    #[serde(default)]
    pub(crate) offset: Option<i64>,
    #[serde(default)]
    pub(crate) limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SessionToolUseStatsArgs {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) top_n: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SetSessionGoalArgs {
    pub(crate) session_id: String,
    #[serde(default)]
    pub(crate) name: Option<String>,
    #[serde(default)]
    pub(crate) role: Option<String>,
    #[serde(default)]
    pub(crate) instructions: Option<String>,
    #[serde(default)]
    pub(crate) goal: Option<String>,
    #[serde(default)]
    pub(crate) next_steps: Option<String>,
    #[serde(default)]
    pub(crate) set_by: Option<String>,
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    match name {
        TOOL_LIST_SESSION_TOOL_USES => {
            let args = parse_args_required::<ListSessionToolUsesArgs>(arguments)?;
            let value = client
                .list_session_tool_uses(
                    &args.id,
                    crate::client::ListSessionToolUsesQuery {
                        offset: args.offset,
                        limit: args.limit,
                        order: args.order.as_deref(),
                        include_results: args.include_results,
                        agent: args.agent.as_deref(),
                    },
                )
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("tool_uses", value))
        }
        TOOL_CONFIRM_MESSAGE_RECEIPT => {
            let args = parse_args_required::<ConfirmMessageReceiptArgs>(arguments)?;
            let value = client
                .confirm_message_receipt(&args.session_id, &args.message_uuid)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("receipt", value))
        }
        TOOL_LIST_FILE_WRITES => {
            let args = parse_args_default::<ListFileWritesArgs>(arguments)?;
            let value = client
                .list_file_writes(&crate::client::FileWritesQuery {
                    project: args.project.as_deref(),
                    from: args.from.as_deref(),
                    to: args.to.as_deref(),
                    path: args.path.as_deref(),
                    collisions_only: args.collisions_only,
                    offset: args.offset,
                    limit: args.limit,
                })
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("file_writes", value))
        }
        TOOL_SESSION_TOOL_USE_STATS => {
            let args = parse_args_required::<SessionToolUseStatsArgs>(arguments)?;
            let value = client
                .session_tool_use_stats(&args.id, args.top_n)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("stats", value))
        }
        TOOL_SET_SESSION_GOAL => {
            let args = parse_args_required::<SetSessionGoalArgs>(arguments)?;
            let value = client
                .set_session_goal(crate::client::SetSessionIdentity {
                    session_id: &args.session_id,
                    name: args.name.as_deref(),
                    role: args.role.as_deref(),
                    goal: args.goal.as_deref(),
                    next_steps: args.next_steps.as_deref(),
                    instructions: args.instructions.as_deref(),
                    set_by: args.set_by.as_deref(),
                })
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("session", value))
        }
        other => Err(RmcpError::invalid_params(
            format!("unknown session activity tool: {other}"),
            None,
        )),
    }
}
