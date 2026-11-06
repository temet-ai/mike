use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{object_schema, parse_args_default, parse_args_required, wrap_success};
use crate::client::{InspectClient, TurnVerdictsQuery};
use crate::error::McpError;

pub const TOOL_GET_TURN_VERDICTS: &str = "get_turn_verdicts";
pub const TOOL_LIST_TURN_VERDICTS: &str = "list_turn_verdicts";

pub const TURN_VERDICT_TOOL_NAMES: &[&str] = &[TOOL_GET_TURN_VERDICTS, TOOL_LIST_TURN_VERDICTS];

const MODES: &str = "live (the production judge, whose label is the stored outcome), shadow (a \
     candidate judging a sample of live turns) or eval (an offline eval run item). Shadow and \
     eval verdicts are never the stored label.";

const MODE_FILTER: &str = "Only verdicts of this mode: live, shadow or eval.";

pub fn turn_verdict_tools() -> Vec<Tool> {
    vec![get_turn_verdicts_tool(), list_turn_verdicts_tool()]
}

pub fn owns(name: &str) -> bool {
    TURN_VERDICT_TOOL_NAMES.contains(&name)
}

fn text(description: &str) -> Value {
    json!({"type": "string", "description": description})
}

fn integer(description: &str) -> Value {
    json!({"type": "integer", "description": description})
}

fn page_properties() -> Vec<(&'static str, Value)> {
    vec![
        ("limit", integer("Rows per page, 1 to 500, default 50.")),
        ("offset", integer("Rows to skip, default 0.")),
    ]
}

fn get_turn_verdicts_tool() -> Tool {
    let mut properties = vec![
        ("turn_id", integer("Operator turn id.")),
        ("mode", text(MODE_FILTER)),
    ];
    properties.extend(page_properties());
    Tool::new(
        Cow::Borrowed(TOOL_GET_TURN_VERDICTS),
        Cow::Owned(format!(
            "Every verdict recorded for one operator turn, newest first, each whole: the rubric \
             and input-builder versions, the bundle, the verdict fields, the exact input text \
             the judge was sent, and the call's cost and tokens. mode filters to {MODES}"
        )),
        object_schema(properties, &["turn_id"]),
    )
}

fn list_turn_verdicts_tool() -> Tool {
    let mut properties = vec![
        ("mode", text(MODE_FILTER)),
        (
            "bundle_id",
            integer("Only verdicts made under this eval bundle."),
        ),
        (
            "rubric_version",
            text("Only verdicts stamped with this effective rubric version."),
        ),
        ("session_id", text("Session UUID.")),
        (
            "since",
            text("RFC 3339; verdicts recorded at or after this moment."),
        ),
        (
            "until",
            text("RFC 3339; verdicts recorded before this moment."),
        ),
    ];
    properties.extend(page_properties());
    Tool::new(
        Cow::Borrowed(TOOL_LIST_TURN_VERDICTS),
        Cow::Owned(format!(
            "Turn verdicts across sessions, newest first, whole rows with a total. Filter by \
             mode, bundle, rubric version, session and time window. mode is {MODES}"
        )),
        object_schema(properties, &[]),
    )
}

#[derive(Debug, Deserialize)]
struct GetTurnVerdictsArgs {
    turn_id: i64,
    mode: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
struct ListTurnVerdictsArgs {
    mode: Option<String>,
    bundle_id: Option<i64>,
    rubric_version: Option<String>,
    session_id: Option<String>,
    since: Option<String>,
    until: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn get_turn_verdicts(
    client: &InspectClient,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    let args = parse_args_required::<GetTurnVerdictsArgs>(arguments)?;
    let query = TurnVerdictsQuery {
        mode: args.mode.as_deref(),
        limit: args.limit,
        offset: args.offset,
        ..TurnVerdictsQuery::default()
    };
    let value = client
        .get_turn_verdicts(args.turn_id, &query)
        .await
        .map_err(McpError::into_rmcp)?;
    Ok(wrap_success("turn_verdicts", value))
}

async fn list_turn_verdicts(
    client: &InspectClient,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    let args = parse_args_default::<ListTurnVerdictsArgs>(arguments)?;
    let query = TurnVerdictsQuery {
        mode: args.mode.as_deref(),
        bundle_id: args.bundle_id,
        rubric_version: args.rubric_version.as_deref(),
        session_id: args.session_id.as_deref(),
        since: args.since.as_deref(),
        until: args.until.as_deref(),
        limit: args.limit,
        offset: args.offset,
    };
    let value = client
        .list_turn_verdicts(&query)
        .await
        .map_err(McpError::into_rmcp)?;
    Ok(wrap_success("turn_verdicts", value))
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    match name {
        TOOL_GET_TURN_VERDICTS => get_turn_verdicts(client, arguments).await,
        TOOL_LIST_TURN_VERDICTS => list_turn_verdicts(client, arguments).await,
        _ => Err(RmcpError::invalid_params(
            format!("unknown turn verdict tool: {name}"),
            None,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{TOOL_GET_TURN_VERDICTS, TOOL_LIST_TURN_VERDICTS, turn_verdict_tools};

    #[test]
    fn get_turn_verdicts_requires_a_turn_id_and_list_requires_nothing() {
        let tools = turn_verdict_tools();
        let required = |name: &str| {
            tools
                .iter()
                .find(|t| t.name == name)
                .and_then(|t| t.input_schema.get("required").cloned())
        };
        assert_eq!(
            required(TOOL_GET_TURN_VERDICTS),
            Some(serde_json::json!(["turn_id"]))
        );
        let list = required(TOOL_LIST_TURN_VERDICTS);
        assert!(list.is_none() || list == Some(serde_json::json!([])));
    }

    #[test]
    fn both_tools_name_the_three_modes() {
        for tool in turn_verdict_tools() {
            let description = tool.description.as_deref().unwrap_or_default();
            for mode in ["live", "shadow", "eval"] {
                assert!(description.contains(mode), "{} omits {mode}", tool.name);
            }
        }
    }
}
