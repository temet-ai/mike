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

pub const SESSIONS_TOOL_NAMES: &[&str] = &[
    TOOL_LIST_SESSIONS,
    TOOL_GET_SESSION,
    TOOL_GET_SESSION_HEAD,
    TOOL_GET_SESSION_THREADS,
    TOOL_LIST_PROJECTS,
];

pub const TOOL_LIST_SESSIONS: &str = "list_sessions";
pub const TOOL_GET_SESSION: &str = "get_session";
pub const TOOL_GET_SESSION_HEAD: &str = "get_session_head";
pub const TOOL_GET_SESSION_THREADS: &str = "get_session_threads";
pub const TOOL_LIST_PROJECTS: &str = "list_projects";

pub fn sessions_tools() -> Vec<Tool> {
    vec![
        list_sessions_tool(),
        get_session_tool(),
        get_session_head_tool(),
        get_session_threads_tool(),
        list_projects_tool(),
    ]
}

pub fn owns(name: &str) -> bool {
    SESSIONS_TOOL_NAMES.contains(&name)
}

fn list_sessions_tool() -> Tool {
    let schema = object_schema(
        vec![
            ("order", order_property()),
            (
                "date_from",
                json!({
                    "type": "string",
                    "description": "RFC 3339 lower bound on `last_seen` (inclusive). Absent → 7-day default window."
                }),
            ),
            (
                "date_to",
                json!({
                    "type": "string",
                    "description": "RFC 3339 upper bound on `last_seen` (exclusive). Absent → now (paired with 7-day default)."
                }),
            ),
            (
                "min_request_count",
                json!({
                    "type": "integer",
                    "minimum": 0,
                    "description": "Only sessions with at least this many requests."
                }),
            ),
            (
                "project",
                json!({
                    "type": "string",
                    "description": "Substring match (case-insensitive) against the project path."
                }),
            ),
            (
                "fields",
                json!({
                    "type": "string",
                    "description": "Comma-separated SessionSummary field names to return, e.g. \"id,request_count,project\". Absent returns the full row."
                }),
            ),
            (
                "limit",
                json!({
                    "type": "integer",
                    "minimum": 1,
                    "description": "Maximum sessions returned. Default 50, max 200; the default applies when omitted."
                }),
            ),
            (
                "offset",
                json!({
                    "type": "integer",
                    "minimum": 0,
                    "description": "Pagination start — skip this many matching sessions before returning results. Default 0."
                }),
            ),
        ],
        &[],
    );
    Tool::new(
        Cow::Borrowed(TOOL_LIST_SESSIONS),
        Cow::Borrowed(
            "List the sessions in the store, sorted by last activity: newest first by \
             default (`order=desc`), `order=asc` for oldest first. Default 50 rows per \
             page, max 200.",
        ),
        schema,
    )
}

fn get_session_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "id",
                json!({
                    "type": "string",
                    "description": "The session id (a canonical UUID from the /api/sessions list)."
                }),
            ),
            (
                "offset",
                json!({
                    "type": "integer",
                    "minimum": 0,
                    "description": "Skip this many requests — counted in the selected `order` direction (default desc = newest first). Default 0."
                }),
            ),
            (
                "limit",
                json!({
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 500,
                    "description": "Maximum number of requests to return (default 50, max 500). Summary metadata is always untruncated."
                }),
            ),
            ("order", order_property()),
        ],
        &["id"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_GET_SESSION),
        Cow::Borrowed(
            "Fetch a session with its paginated request list, newest first by default. \
             Returns total_requests plus the applied offset and limit.",
        ),
        schema,
    )
}

fn get_session_head_tool() -> Tool {
    let schema = object_schema(
        vec![(
            "session_id",
            json!({
                "type": "string",
                "description": "Session id (UUID from list_sessions)."
            }),
        )],
        &["session_id"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_GET_SESSION_HEAD),
        Cow::Borrowed(
            "Light polling variant of get_session. Returns only: session_id, last_status, \
             newest_request_id, newest_request_number, originating_operator_prompt \
             (the user_prompt of the first operator-kind turn; null until one exists), \
             prompt_kind_counts (turn count per prompt_kind code: operator, activity-label, \
             hook-feedback, teammate-message, session-recap, background-event, compaction, \
             analysis-probe; every code is present, 0 when none). \
             No requests array, no tool_names.",
        ),
        schema,
    )
}

fn get_session_threads_tool() -> Tool {
    let schema = object_schema(
        vec![(
            "session_id",
            json!({
                "type": "string",
                "description": "Session UUID whose threads to list — every thread it has sent to or received from, as sender or recipient of any message in it."
            }),
        )],
        &["session_id"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_GET_SESSION_THREADS),
        Cow::Borrowed(
            "List every mailbox thread this session participates in, newest \
             `last_message_at` first. Read-only. Each entry: `thread_uuid`, `subject`, \
             `created_at`, `last_message_at`, `resolved_at` (null until the thread is \
             resolved), `created_by_session_uuid`, `participants` (every session UUID that \
             has sent or received a message in it), `tags` (the codes set with \
             send_session_message's `tags` argument), `message_count`, `unread_count` \
             (delivered, not yet acknowledged), `worst_delivery_state` (the status of the \
             least-progressed message: pending | claimed | delivered | consumed | \
             dead_lettered) and `has_stale_pending` (a message has sat undelivered past the \
             staleness threshold). There is no MCP tool for a thread's full transcript. \
             Append to a thread with send_session_message(thread_id=...).",
        ),
        schema,
    )
}

fn list_projects_tool() -> Tool {
    let schema = object_schema(vec![], &[]);
    Tool::new(
        Cow::Borrowed(TOOL_LIST_PROJECTS),
        Cow::Borrowed(
            "List all known projects: project_uuid, first_seen, last_seen and pinned. No \
             session or request counts; read a single project by uuid or path for those. \
             Includes projects defined deliberately that have no captured traffic yet. \
             Returns an array ordered by last_seen descending.",
        ),
        schema,
    )
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct ListSessionsArgs {
    #[serde(default)]
    pub(crate) order: Option<String>,
    #[serde(default)]
    pub(crate) date_from: Option<String>,
    #[serde(default)]
    pub(crate) date_to: Option<String>,
    #[serde(default)]
    pub(crate) min_request_count: Option<i64>,
    #[serde(default)]
    pub(crate) project: Option<String>,
    #[serde(default)]
    pub(crate) fields: Option<String>,
    #[serde(default)]
    pub(crate) limit: Option<u64>,
    #[serde(default)]
    pub(crate) offset: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GetSessionArgs {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) offset: Option<u64>,
    #[serde(default)]
    pub(crate) limit: Option<u64>,
    #[serde(default)]
    pub(crate) order: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GetSessionHeadArgs {
    pub(crate) session_id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GetSessionThreadsArgs {
    pub(crate) session_id: String,
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    match name {
        TOOL_LIST_SESSIONS => {
            let args = parse_args_default::<ListSessionsArgs>(arguments)?;
            let mut query: Vec<(&str, String)> = Vec::new();
            if let Some(ref ord) = args.order {
                query.push(("order", ord.clone()));
            }
            if let Some(ref df) = args.date_from {
                query.push(("date_from", df.clone()));
            }
            if let Some(ref dt) = args.date_to {
                query.push(("date_to", dt.clone()));
            }
            if let Some(n) = args.min_request_count {
                query.push(("min_request_count", n.to_string()));
            }
            if let Some(ref p) = args.project {
                query.push(("project", p.clone()));
            }
            if let Some(ref f) = args.fields {
                query.push(("fields", f.clone()));
            }
            if let Some(l) = args.limit {
                query.push(("limit", l.to_string()));
            }
            if let Some(o) = args.offset {
                query.push(("offset", o.to_string()));
            }
            let (value, note) = client
                .get_json_with_note("/api/sessions", &query)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success_with_note("sessions", value, note))
        }
        TOOL_GET_SESSION => {
            let args = parse_args_required::<GetSessionArgs>(arguments)?;
            let value = client
                .get_session(&args.id, args.offset, args.limit, args.order.as_deref())
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("session", value))
        }
        TOOL_GET_SESSION_HEAD => {
            let args = parse_args_required::<GetSessionHeadArgs>(arguments)?;
            let value = client
                .get_session_head(&args.session_id)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("head", value))
        }
        TOOL_GET_SESSION_THREADS => {
            let args = parse_args_required::<GetSessionThreadsArgs>(arguments)?;
            let value = client
                .get_session_threads(&args.session_id)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("threads", value))
        }
        TOOL_LIST_PROJECTS => {
            let value = client.list_projects().await.map_err(McpError::into_rmcp)?;
            Ok(wrap_success("projects", value))
        }
        other => Err(RmcpError::invalid_params(
            format!("unknown session tool: {other}"),
            None,
        )),
    }
}
