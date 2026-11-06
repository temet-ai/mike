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

pub const TEXT_SEARCH_TOOL_NAMES: &[&str] = &[
    TOOL_FIND_TEXT,
    TOOL_SEARCH_SESSION_CONTENT,
    TOOL_LIST_TOOL_ERRORS,
];

pub const TOOL_FIND_TEXT: &str = "find_text";
pub const TOOL_SEARCH_SESSION_CONTENT: &str = "search_session_content";
pub const TOOL_LIST_TOOL_ERRORS: &str = "list_tool_errors";

pub fn text_search_tools() -> Vec<Tool> {
    vec![
        find_text_tool(),
        search_session_content_tool(),
        list_tool_errors_tool(),
    ]
}

pub fn owns(name: &str) -> bool {
    TEXT_SEARCH_TOOL_NAMES.contains(&name)
}

fn find_text_properties() -> Vec<(&'static str, Value)> {
    vec![
        (
            "needle",
            json!({
                "type": "string",
                "minLength": 1,
                "description": "Substring to search for. Required; empty string is a 400."
            }),
        ),
        (
            "request_id",
            json!({
                "type": "integer",
                "minimum": 0,
                "description": "Scope to a single request. Fast path — only that one body is scanned."
            }),
        ),
        (
            "session_id",
            json!({
                "type": "string",
                "description": "Scope to a session (UUID). All bodies in the session are scanned."
            }),
        ),
        (
            "source",
            json!({
                "type": "string",
                "enum": ["request", "request_sent", "response"],
                "description": "Which captured body to search. Default `request`. Without request_id or session_id only `response` is permitted; use recall_similar_turns for unscoped request-body search."
            }),
        ),
        (
            "case_sensitive",
            json!({
                "type": "boolean",
                "description": "Default true. ASCII-fold lowercase compare when false."
            }),
        ),
        (
            "context_bytes",
            json!({
                "type": "integer",
                "minimum": 0,
                "maximum": 4096,
                "description": "Bytes of UTF-8-safe context returned on each side of every match. Default 256, max 4096."
            }),
        ),
        (
            "max_matches",
            json!({
                "type": "integer",
                "minimum": 1,
                "maximum": 500,
                "description": "Cap on returned matches. Default 50, max 500. `full_match_count` is the honest total for the scanned scope — null while `has_more` is true (see `cursor`)."
            }),
        ),
        (
            "date_from",
            json!({
                "type": "string",
                "description": "RFC 3339 lower bound for the `created_at` search window. Applied when no id scope is given; defaults to 7 days ago when both absent."
            }),
        ),
        (
            "date_to",
            json!({
                "type": "string",
                "description": "RFC 3339 upper bound for the `created_at` search window."
            }),
        ),
        (
            "cursor",
            json!({
                "type": "integer",
                "minimum": 0,
                "description": "Session scope only: resume scanning after this request id. Pass the previous call's `next_cursor` when `has_more` was true."
            }),
        ),
    ]
}

fn find_text_tool() -> Tool {
    let schema = object_schema(find_text_properties(), &["needle"]);
    Tool::new(
        Cow::Borrowed(TOOL_FIND_TEXT),
        Cow::Borrowed(
            "Substring search over the raw captured bytes of request, request_sent or response bodies: \
             the only way to find text that appears solely inside a tool call, tool result, thinking block \
             or system prompt. Scope: `request_id` scans that one request, complete in one call; \
             `session_id` scans the session, bounded per call (while `has_more` is true, pass \
             `next_cursor` back as `cursor`); neither scans response bodies only (source=response, 7-day \
             default window, widen with date_from and date_to), so pass session_id or request_id to search \
             request bodies. Returns matches {request_id, session_id, source, byte_offset, match_length, \
             line, column, before, matched, after}. `full_match_count` is the total for the scope scanned, \
             or null while `has_more` is true; a 0 is not absence until `has_more` is false. Binary \
             bodies give full_match_count 0, which is complete. For the conversation itself (operator \
             prompts and assistant replies) use search_session_content.",
        ),
        schema,
    )
}

fn search_session_content_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "q",
                json!({
                    "type": "string",
                    "minLength": 1,
                    "description": "Substring to search for in the conversation (operator prompts + assistant replies). Required; empty string is a 400."
                }),
            ),
            (
                "project",
                json!({
                    "type": "string",
                    "description": "Restrict to sessions under this project path."
                }),
            ),
            (
                "limit",
                json!({
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 100,
                    "description": "Max SESSIONS per page. Default 20, max 100 — this paginates sessions, not matches."
                }),
            ),
            (
                "offset",
                json!({
                    "type": "integer",
                    "minimum": 0,
                    "description": "Skip this many sessions before returning results."
                }),
            ),
        ],
        &["q"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_SEARCH_SESSION_CONTENT),
        Cow::Borrowed(
            "Literal search over the conversation (operator prompts and assistant replies, matched in \
             chunks of at most 1500 characters), grouped by session; use it first for 'which session \
             discussed X'. It does not search tool-call inputs or results, thinking blocks or system \
             prompts: use find_text for those. Returns {query, total_matches, total_sessions, sessions, \
             has_more, next_offset}; `total_matches` and `total_sessions` are corpus-wide totals \
             independent of paging. Each session: {session_id, project_path, project_display_name, title, \
             match_count, turn_count, last_match_at, matches, matches_truncated}. Each match: {turn_id, \
             request_id, role, chunk_idx, byte_offset, match_length, line, column, before, matched, after, \
             created_at}. `matches_truncated=true` means the session has more matching chunks than are \
             returned: narrow `q`, or use find_text or get_session_operator_turns for the rest.",
        ),
        schema,
    )
}

fn list_tool_errors_tool() -> Tool {
    let schema = object_schema(
        vec![
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
                    "description": "Maximum rows per page (default 100, max 500)."
                }),
            ),
            (
                "offset",
                json!({
                    "type": "integer",
                    "minimum": 0,
                    "description": "Skip this many rows before returning. Default 0. Prefer `cursor`."
                }),
            ),
            (
                "cursor",
                json!({
                    "type": "string",
                    "description": "`next_cursor` from the previous page; returns the rows after it. Overrides `offset`."
                }),
            ),
            (
                "project",
                json!({
                    "type": "string",
                    "description": "Optional case-insensitive substring filter on `projects.projects.project_path` (matches `list_sessions`)."
                }),
            ),
            (
                "tool_name",
                json!({
                    "type": "string",
                    "description": "Optional exact-match filter on `tools.uses.tool_name`."
                }),
            ),
            ("order", order_property()),
        ],
        &[],
    );
    Tool::new(
        Cow::Borrowed(TOOL_LIST_TOOL_ERRORS),
        Cow::Borrowed(
            "Cross-request feed of tool_result blocks with is_error=true. \
             Each row: tool_name, session_id, project_path, full content (never sliced). \
             Paginated, newest-first.",
        ),
        schema,
    )
}

#[derive(Debug, Deserialize)]
pub(crate) struct FindTextArgs {
    pub(crate) needle: String,
    #[serde(default)]
    pub(crate) request_id: Option<u64>,
    #[serde(default)]
    pub(crate) session_id: Option<String>,
    #[serde(default)]
    pub(crate) source: Option<String>,
    #[serde(default)]
    pub(crate) case_sensitive: Option<bool>,
    #[serde(default)]
    pub(crate) context_bytes: Option<u32>,
    #[serde(default)]
    pub(crate) max_matches: Option<u32>,
    #[serde(default)]
    pub(crate) date_from: Option<String>,
    #[serde(default)]
    pub(crate) date_to: Option<String>,
    #[serde(default)]
    pub(crate) cursor: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SearchSessionContentArgs {
    pub(crate) q: String,
    #[serde(default)]
    pub(crate) project: Option<String>,
    #[serde(default)]
    pub(crate) limit: Option<u32>,
    #[serde(default)]
    pub(crate) offset: Option<u32>,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct ListToolErrorsArgs {
    #[serde(default)]
    pub(crate) since: Option<String>,
    #[serde(default)]
    pub(crate) date_from: Option<String>,
    #[serde(default)]
    pub(crate) date_to: Option<String>,
    #[serde(default)]
    pub(crate) limit: Option<u64>,
    #[serde(default)]
    pub(crate) offset: Option<u64>,
    #[serde(default)]
    pub(crate) project: Option<String>,
    #[serde(default)]
    pub(crate) tool_name: Option<String>,
    #[serde(default)]
    pub(crate) order: Option<String>,
    #[serde(default)]
    pub(crate) cursor: Option<String>,
}

fn find_text_query(args: &FindTextArgs) -> Vec<(&'static str, String)> {
    let mut query: Vec<(&'static str, String)> = Vec::with_capacity(9);
    query.push(("needle", args.needle.clone()));
    if let Some(rid) = args.request_id {
        query.push(("request_id", rid.to_string()));
    }
    if let Some(ref sid) = args.session_id {
        query.push(("session_id", sid.clone()));
    }
    if let Some(ref s) = args.source {
        query.push(("source", s.clone()));
    }
    if let Some(cs) = args.case_sensitive {
        query.push(("case_sensitive", cs.to_string()));
    }
    if let Some(c) = args.context_bytes {
        query.push(("context_bytes", c.to_string()));
    }
    if let Some(m) = args.max_matches {
        query.push(("max_matches", m.to_string()));
    }
    if let Some(ref df) = args.date_from {
        query.push(("date_from", df.clone()));
    }
    if let Some(ref dt) = args.date_to {
        query.push(("date_to", dt.clone()));
    }
    if let Some(c) = args.cursor {
        query.push(("cursor", c.to_string()));
    }
    query
}

fn search_session_content_query(args: &SearchSessionContentArgs) -> Vec<(&'static str, String)> {
    let mut query: Vec<(&'static str, String)> = Vec::with_capacity(4);
    query.push(("q", args.q.clone()));
    if let Some(ref p) = args.project {
        query.push(("project", p.clone()));
    }
    if let Some(l) = args.limit {
        query.push(("limit", l.to_string()));
    }
    if let Some(o) = args.offset {
        query.push(("offset", o.to_string()));
    }
    query
}

fn list_tool_errors_query(args: &ListToolErrorsArgs) -> Vec<(&'static str, String)> {
    let mut q: Vec<(&'static str, String)> = Vec::new();
    if let Some(ref s) = args.since {
        q.push(("since", s.clone()));
    }
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
    if let Some(ref p) = args.project {
        q.push(("project", p.clone()));
    }
    if let Some(ref t) = args.tool_name {
        q.push(("tool_name", t.clone()));
    }
    if let Some(ref ord) = args.order {
        q.push(("order", ord.clone()));
    }
    if let Some(ref c) = args.cursor {
        q.push(("cursor", c.clone()));
    }
    q
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    match name {
        TOOL_FIND_TEXT => {
            let args = parse_args_required::<FindTextArgs>(arguments)?;
            let query = find_text_query(&args);
            let value = client
                .get_json("/api/body/find", &query)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("find", value))
        }
        TOOL_SEARCH_SESSION_CONTENT => {
            let args = parse_args_required::<SearchSessionContentArgs>(arguments)?;
            let query = search_session_content_query(&args);
            let value = client
                .get_json("/api/content/search", &query)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("content_search", value))
        }
        TOOL_LIST_TOOL_ERRORS => {
            let args = parse_args_default::<ListToolErrorsArgs>(arguments)?;
            let q = list_tool_errors_query(&args);
            let (value, note) = client
                .get_json_with_note("/api/tool-results/errors", &q)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success_with_note("tool_errors", value, note))
        }
        other => Err(RmcpError::invalid_params(
            format!("unknown search tool: {other}"),
            None,
        )),
    }
}
