use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use super::{object_schema, parse_args_default, parse_args_required, wrap_success};
use crate::client::InspectClient;
use crate::error::McpError;

pub const TOOL_WAIT_FOR: &str = "wait_for";
pub const TOOL_LIST_CONDITIONS: &str = "list_conditions";
pub const TOOL_CANCEL_CONDITION: &str = "cancel_condition";

pub const CONDITION_TOOL_NAMES: &[&str] =
    &[TOOL_WAIT_FOR, TOOL_LIST_CONDITIONS, TOOL_CANCEL_CONDITION];

pub fn condition_tools() -> Vec<Tool> {
    vec![
        wait_for_tool(),
        list_conditions_tool(),
        cancel_condition_tool(),
    ]
}

pub fn owns(name: &str) -> bool {
    CONDITION_TOOL_NAMES.contains(&name)
}

const WAIT_FOR_DESCRIPTION: &str = "Register what this session is waiting for and what mike does \
when it holds, then end your turn instead of polling or sleeping. Mutating: it stores one row and \
nothing runs until the condition holds.\n\
Kinds (`kind`, with `spec`): `time` ({}; set after_secs or not_before); `session_turn_ended` \
({session_uuid}, defaults to your own session_id); `session_ended` ({session_uuid}); \
`fleet_event` ({run_id, kind, task?, session_uuid?}, kind one of the fleet event kinds such as \
check_finished or commit_landed); `sql` ({query, expect: \"rows\" | \"no_rows\"}: one read-only \
SELECT, no semicolon, at most 4000 characters, run in a read-only transaction every \
check_every_secs, 60 or more). Events older than not_before do not count.\n\
Actions (`action`): `return` (default) pushes `brief` to the target session if it is open and \
active within the hour, resumes it if it closed within the hour, otherwise starts a new session \
(also for an open session idle longer when `thread` is set, so its cold cache is not rewritten); `start` starts a new \
session in `folder` (or the target's folder) with `brief`; `message` mails `target_session`. \
`target_session` defaults to your own session_id. Pass `thread` (a work thread slug, with \
`project` or your session_id) to add that thread's state and newest brief to what the session \
receives.\n\
A condition that already holds fires on the next pass. With `deadline` or `deadline_secs`, one \
that has not held by then fires anyway, marked missed.\n\
Examples: 24 h later start a session on a work thread: {\"kind\": \"time\", \"after_secs\": \
86400, \"action\": \"start\", \"folder\": \"/repo\", \"thread\": \"side-calls\", \"session_id\": \
\"<you>\", \"brief\": \"Continue the side-calls follow-up.\"}. When session S's turn ends return \
to me: {\"kind\": \"session_turn_ended\", \"spec\": {\"session_uuid\": \"<S>\"}, \"session_id\": \
\"<you>\", \"brief\": \"S finished a turn; read its result.\"}.\n\
Returns the stored condition with its condition_uuid; list_conditions shows state and outcome, \
cancel_condition stops an armed one.";

fn wait_for_properties() -> Vec<(&'static str, Value)> {
    vec![
        (
            "kind",
            json!({
                "type": "string",
                "enum": ["time", "session_turn_ended", "session_ended", "fleet_event", "sql"],
                "description": "What is waited on; its `spec` shape is in the tool description."
            }),
        ),
        (
            "spec",
            json!({
                "type": "object",
                "description": "The kind's parameters, e.g. {\"session_uuid\": \"...\"} or \
                    {\"query\": \"SELECT 1 FROM ...\", \"expect\": \"rows\"}. Omit for `time`."
            }),
        ),
        (
            "session_id",
            json!({
                "type": "string",
                "description": "Your own session id (a canonical UUID): the registering session, \
                    the default target and the default `session_uuid` for session kinds."
            }),
        ),
        (
            "after_secs",
            json!({
                "type": "integer",
                "description": "Start counting this many seconds from now (not_before)."
            }),
        ),
        (
            "not_before",
            json!({
                "type": "string",
                "description": "RFC 3339 time before which the condition is not evaluated and \
                    earlier events do not count. Overrides after_secs."
            }),
        ),
        (
            "deadline_secs",
            json!({
                "type": "integer",
                "description": "Fire as missed if the condition has not held this many seconds \
                    from now."
            }),
        ),
        (
            "deadline",
            json!({
                "type": "string",
                "description": "RFC 3339 deadline; overrides deadline_secs."
            }),
        ),
    ]
}

fn wait_for_action_properties() -> Vec<(&'static str, Value)> {
    vec![
        (
            "action",
            json!({
                "type": "string",
                "enum": ["return", "start", "message"],
                "description": "What mike does when the condition holds. Default `return`."
            }),
        ),
        (
            "target_session",
            json!({
                "type": "string",
                "description": "Session to return to or mail. Defaults to session_id."
            }),
        ),
        (
            "folder",
            json!({
                "type": "string",
                "description": "Folder a started session opens in. Defaults to the target \
                    session's last folder."
            }),
        ),
        (
            "brief",
            json!({
                "type": "string",
                "description": "What the woken session should do, written as if to someone \
                    without your context. Required."
            }),
        ),
        (
            "thread",
            json!({
                "type": "string",
                "description": "Work thread slug whose state and newest brief are added to the \
                    message. Needs `project` or session_id."
            }),
        ),
        (
            "project",
            json!({
                "type": "string",
                "description": "Project path or UUID the thread belongs to."
            }),
        ),
        (
            "check_every_secs",
            json!({
                "type": "integer",
                "description": "Minimum seconds between evaluations of a `sql` condition. \
                    Default 300, minimum 60."
            }),
        ),
    ]
}

fn wait_for_tool() -> Tool {
    let mut properties = wait_for_properties();
    properties.extend(wait_for_action_properties());
    Tool::new(
        Cow::Borrowed(TOOL_WAIT_FOR),
        Cow::Borrowed(WAIT_FOR_DESCRIPTION),
        object_schema(properties, &["kind", "brief"]),
    )
}

fn list_conditions_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "thread",
                json!({"type": "string", "description": "Only conditions on this work thread."}),
            ),
            (
                "state",
                json!({
                    "type": "string",
                    "enum": ["armed", "fired", "missed", "failed", "cancelled"],
                    "description": "Only conditions in this state."
                }),
            ),
            (
                "session",
                json!({
                    "type": "string",
                    "description": "Only conditions registered by this session id."
                }),
            ),
            (
                "limit",
                json!({"type": "integer", "description": "Rows to return, newest first. Default 50, at most 200."}),
            ),
        ],
        &[],
    );
    Tool::new(
        Cow::Borrowed(TOOL_LIST_CONDITIONS),
        Cow::Borrowed(
            "List registered conditions newest first, as full rows: kind, spec, deadline, action, \
             brief, state (armed, fired, missed, failed or cancelled), when it fired, how the \
             action ran (`outcome`: via push, mail, resume or start, the launch and session) and \
             the last error. Read-only. Use it to see what is still armed on a thread or for \
             your session, and to check a wait_for result.",
        ),
        schema,
    )
}

fn cancel_condition_tool() -> Tool {
    let schema = object_schema(
        vec![(
            "condition_uuid",
            json!({
                "type": "string",
                "description": "The condition_uuid returned by wait_for or list_conditions."
            }),
        )],
        &["condition_uuid"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_CANCEL_CONDITION),
        Cow::Borrowed(
            "Stop an armed condition so its action never runs. Mutating. A condition that already \
             fired, missed, failed or was cancelled is refused with its uuid in the error; read \
             its state with list_conditions.",
        ),
        schema,
    )
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct WaitForArgs {
    pub(crate) kind: String,
    pub(crate) brief: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) spec: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) after_secs: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) not_before: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) deadline_secs: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) deadline: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) target_session: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) folder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) thread: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) check_every_secs: Option<i32>,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct ListConditionsArgs {
    #[serde(default)]
    pub(crate) thread: Option<String>,
    #[serde(default)]
    pub(crate) state: Option<String>,
    #[serde(default)]
    pub(crate) session: Option<String>,
    #[serde(default)]
    pub(crate) limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CancelConditionArgs {
    pub(crate) condition_uuid: String,
}

pub(crate) fn list_query(args: ListConditionsArgs) -> Vec<(&'static str, String)> {
    let mut query = Vec::new();
    if let Some(thread) = args.thread {
        query.push(("thread", thread));
    }
    if let Some(state) = args.state {
        query.push(("state", state));
    }
    if let Some(session) = args.session {
        query.push(("session", session));
    }
    if let Some(limit) = args.limit {
        query.push(("limit", limit.to_string()));
    }
    query
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    match name {
        TOOL_WAIT_FOR => {
            let args = parse_args_required::<WaitForArgs>(arguments)?;
            let body = serde_json::to_value(&args)
                .map_err(|e| RmcpError::invalid_params(format!("invalid arguments: {e}"), None))?;
            let value = client
                .create_condition(&body)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("condition", value))
        }
        TOOL_LIST_CONDITIONS => {
            let args = parse_args_default::<ListConditionsArgs>(arguments)?;
            let value = client
                .list_conditions(&list_query(args))
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("conditions", value))
        }
        TOOL_CANCEL_CONDITION => {
            let args = parse_args_required::<CancelConditionArgs>(arguments)?;
            let value = client
                .cancel_condition(&args.condition_uuid)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("result", value))
        }
        other => Err(RmcpError::invalid_params(
            format!("unknown condition tool: {other}"),
            None,
        )),
    }
}

#[cfg(test)]
#[path = "conditions_tests.rs"]
mod tests;
