use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::text_search_find::{FIND_TEXT_SETTLE_BUDGET, find_text_settled};
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

#[cfg(test)]
#[path = "text_search_tests.rs"]
mod text_search_tests;

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
                "description": "Scope to a session (UUID). The whole session is scanned in one call, or the answer says how far it got."
            }),
        ),
        (
            "source",
            json!({
                "type": "string",
                "enum": ["request", "request_sent", "response"],
                "description": "Which captured text to search. Default `request` with request_id or session_id, `response` without either. Asking for `request` without a scope is an error; use recall_similar_turns to find a session by content."
            }),
        ),
        (
            "whole_bodies",
            json!({
                "type": "boolean",
                "description": "Default false. Session scope with source=request only: true scans every request body in full, so history resent in each later request, system reminders included, is matched again in every request. Ignored for other sources, which always scan bodies."
            }),
        ),
        (
            "include_reminders",
            json!({
                "type": "boolean",
                "description": "Default false. Session scope with source=request and whole_bodies false: true also counts matches inside <system-reminder> spans. They are skipped by default and counted in `reminder_matches_skipped`."
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
                "description": "Cap on returned matches. Default 50, max 500. Counting continues past the cap: `full_match_count` is the true total when `scan_complete` is true and null otherwise (see `cursor`)."
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
                "description": "Session scope only: resume scanning after this request id. Pass the previous answer's `next_cursor` when it came back with matches and `has_more` true; an empty incomplete scan is continued for you, or returned as an error that names the cursor."
            }),
        ),
    ]
}

fn find_text_tool() -> Tool {
    let schema = object_schema(find_text_properties(), &["needle"]);
    Tool::new(
        Cow::Borrowed(TOOL_FIND_TEXT),
        Cow::Borrowed(
            "Substring search over captured text: the only way to find text that appears solely inside a \
             tool call, tool result, thinking block or system prompt. `session_id` with the default \
             source=request reads the session's messages, each once, at the request where it first \
             appeared (not the whole request bodies, which resend the history every turn); matches carry \
             `message_ord` and `positions` is \"message\", so byte_offset, line and column are positions \
             inside that message's JSON text. Matches inside <system-reminder> spans are skipped unless \
             include_reminders is true, and the calls and results of the search tools (find_text, \
             search_session_content, search_project_knowledge, recall_similar_turns, \
             recall_project_context) that contain the needle are never counted (a response scan drops \
             responses that issued such a call); both are reported in \
             `reminder_matches_skipped` and `own_search_matches_skipped`. whole_bodies=true scans every \
             request body in full instead (`positions` \"body\"). source=response and request_sent always \
             scan bodies. `request_id` scans that one request. With neither id the search reads response \
             bodies (7-day default window, widen with date_from and date_to); asking for source=request \
             without an id is an error. A session scan covers the whole session in one call or reports \
             `scan_complete` false with `requests_scanned`, `requests_total`, `has_more` and `next_cursor` \
             (pass it back as `cursor`). An empty `matches` always means the whole scope was scanned: an \
             incomplete empty scan is continued for up to 30 seconds and then returned as an error naming \
             the cursor. `full_match_count` is the true total when `scan_complete` is true, else null. \
             Matches: {request_id, message_ord (message scans), session_id, source, byte_offset, \
             match_length, line, column, before, matched, after}. Binary bodies give 0, which is complete. \
             For the conversation itself (operator prompts and assistant replies) use \
             search_session_content.",
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
                    "description": "Restrict to one project, given as its project UUID, numeric id, path or name (the values list_project_locations prints). An unknown or ambiguous project is an error that lists the matching projects."
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
            (
                "order",
                json!({
                    "type": "string",
                    "enum": ["matches", "recent"],
                    "description": "Session order. `matches` (default): most matching chunks first, then most matches in operator prompts, then newest. `recent`: newest match first."
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
             prompts: use find_text for those. Sessions come back strongest match first (`order=matches`, \
             the default) or newest first (`order=recent`). Returns {query, order, total_matches, \
             total_sessions, sessions, has_more, next_offset}; `total_matches` and `total_sessions` are \
             corpus-wide totals independent of paging. Each session: {session_id, project_path, \
             project_display_name, title, match_count, prompt_match_count, turn_count, last_match_at, \
             matches, matches_truncated}. Each match: {turn_id, \
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
    #[serde(default)]
    pub(crate) whole_bodies: Option<bool>,
    #[serde(default)]
    pub(crate) include_reminders: Option<bool>,
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
    #[serde(default)]
    pub(crate) order: Option<String>,
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

pub(super) fn find_text_query(args: &FindTextArgs) -> Vec<(&'static str, String)> {
    let mut query: Vec<(&'static str, String)> = Vec::with_capacity(11);
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
    if let Some(whole) = args.whole_bodies {
        query.push(("whole_bodies", whole.to_string()));
    }
    if let Some(include) = args.include_reminders {
        query.push(("include_reminders", include.to_string()));
    }
    query
}

fn search_session_content_query(args: &SearchSessionContentArgs) -> Vec<(&'static str, String)> {
    let mut query: Vec<(&'static str, String)> = Vec::with_capacity(5);
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
    if let Some(ref order) = args.order {
        query.push(("order", order.clone()));
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
            let value = find_text_settled(client, args, FIND_TEXT_SETTLE_BUDGET).await?;
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
