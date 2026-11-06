use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{object_schema, parse_args_default, wrap_success};
use crate::client::{InspectClient, TimeSpentQuery};
use crate::error::McpError;

pub const TIME_SPENT_TOOL_NAMES: &[&str] = &[TOOL_GET_TIME_SPENT];

pub const TOOL_GET_TIME_SPENT: &str = "get_time_spent";

pub fn time_spent_tools() -> Vec<Tool> {
    vec![get_time_spent_tool()]
}

pub fn owns(name: &str) -> bool {
    TIME_SPENT_TOOL_NAMES.contains(&name)
}

fn get_time_spent_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "project",
                json!({
                    "type": "string",
                    "description": "Case-insensitive substring of the project path. Omit for every project."
                }),
            ),
            (
                "project_path",
                json!({
                    "type": "string",
                    "description": "Exact project path, for one project's page. Combines with `project` when both are given. Omit for every project."
                }),
            ),
            (
                "from",
                json!({
                    "type": "string",
                    "description": "RFC 3339 start of the window, inclusive. Default: `to` minus `days`."
                }),
            ),
            (
                "to",
                json!({
                    "type": "string",
                    "description": "RFC 3339 end of the window, exclusive. Default: now. Must be later than `from`."
                }),
            ),
            (
                "days",
                json!({
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 90,
                    "description": "Window length in days, counted back from `to`. Used only when `from` is absent. Default 14."
                }),
            ),
            (
                "include_subagents",
                json!({
                    "type": "boolean",
                    "description": "Count subagent answers as well as main-session answers. Default false."
                }),
            ),
            (
                "session_id",
                json!({
                    "type": "string",
                    "description": "Restrict everything to one session (a canonical UUID from list_sessions)."
                }),
            ),
            (
                "kind",
                json!({
                    "type": "string",
                    "description": "Keep only slow_commands of this kind, e.g. `rust build`, `rust tests`, `gate`, `lint`, `git`, `deploy`, `frontend tests`, `database query`, `waiting on background`, `other shell`. Does not change by_kind or builds_running_at_once."
                }),
            ),
            (
                "limit",
                json!({
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 100,
                    "description": "Number of slow_commands rows to return (default 20, max 100)."
                }),
            ),
            (
                "offset",
                json!({
                    "type": "integer",
                    "minimum": 0,
                    "description": "Number of slow_commands rows to skip, in the same order, to page past `limit`. Default 0."
                }),
            ),
        ],
        &[],
    );
    Tool::new(
        Cow::Borrowed(TOOL_GET_TIME_SPENT),
        Cow::Borrowed(
            "Where sessions spent their time waiting on tools, by kind of work, and how much slower builds \
             ran when other sessions were building at the same time. Returns {from, to, project, project_path, \
             include_subagents, session_id, totals: {wait_seconds, model_seconds, answers}, \
             by_kind: [{kind, answers, answers_over_an_hour, wait_seconds, p50_wait_seconds, \
             p90_wait_seconds, model_seconds, sessions}] (longest wait first), \
             slow_commands: [{command, kind, calls, seconds, p50_seconds, sessions}] (most total \
             seconds first; `command` is the whole command with ids and temp paths normalised, which \
             groups repeats; page with `limit` and `offset`), builds_running_at_once: [{running_at_once, sort_key, \
             calls, seconds, p50_seconds}] (foreground cargo, nextest, make and gate commands grouped by \
             how many other sessions were also building: none, 1 to 2, 3 to 5, 6 or more), notes}. \
             `notes` states how each number is measured. Default window is the last 14 days of \
             main-session answers.",
        ),
        schema,
    )
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct GetTimeSpentArgs {
    #[serde(default)]
    pub(crate) project: Option<String>,
    #[serde(default)]
    pub(crate) project_path: Option<String>,
    #[serde(default)]
    pub(crate) from: Option<String>,
    #[serde(default)]
    pub(crate) to: Option<String>,
    #[serde(default)]
    pub(crate) days: Option<i64>,
    #[serde(default)]
    pub(crate) include_subagents: Option<bool>,
    #[serde(default)]
    pub(crate) session_id: Option<String>,
    #[serde(default)]
    pub(crate) kind: Option<String>,
    #[serde(default)]
    pub(crate) limit: Option<i64>,
    #[serde(default)]
    pub(crate) offset: Option<i64>,
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    match name {
        TOOL_GET_TIME_SPENT => {
            let args = parse_args_default::<GetTimeSpentArgs>(arguments)?;
            let value = client
                .get_time_spent(&TimeSpentQuery {
                    project: args.project.as_deref(),
                    project_path: args.project_path.as_deref(),
                    from: args.from.as_deref(),
                    to: args.to.as_deref(),
                    days: args.days,
                    include_subagents: args.include_subagents,
                    session_id: args.session_id.as_deref(),
                    kind: args.kind.as_deref(),
                    limit: args.limit,
                    offset: args.offset,
                })
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("time_spent", value))
        }
        other => Err(RmcpError::invalid_params(
            format!("unknown time spent tool: {other}"),
            None,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_spent_tools_match_declared_names() {
        let tools = time_spent_tools();
        assert_eq!(tools.len(), TIME_SPENT_TOOL_NAMES.len());
        assert!(owns(TOOL_GET_TIME_SPENT));
        assert!(!owns("list_sessions"));
    }

    #[test]
    fn get_time_spent_schema_declares_every_param_and_requires_none() {
        let tool = get_time_spent_tool();
        let schema = tool.input_schema.as_ref();
        let props = schema
            .get("properties")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        for key in [
            "project",
            "project_path",
            "from",
            "to",
            "days",
            "include_subagents",
            "session_id",
            "kind",
            "limit",
            "offset",
            "select",
        ] {
            assert!(props.contains_key(key), "schema missing {key}");
        }
        assert!(schema.get("required").is_none());
    }

    #[test]
    fn get_time_spent_args_parse_present_fields_and_default_the_rest() -> anyhow::Result<()> {
        let args: GetTimeSpentArgs = serde_json::from_value(json!({"days": 7, "kind": "gate"}))?;
        assert_eq!(args.days, Some(7));
        assert_eq!(args.kind.as_deref(), Some("gate"));
        assert_eq!(args.limit, None);
        Ok(())
    }
}
