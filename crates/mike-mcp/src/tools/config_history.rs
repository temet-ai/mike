use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{object_schema, parse_args_default, parse_args_required, wrap_success};
use crate::client::{ConfigChangesQuery, InspectClient};
use crate::error::McpError;

pub const TOOL_LIST_CONFIG_CHANGES: &str = "list_config_changes";
pub const TOOL_GET_CONFIG_SNAPSHOT: &str = "get_config_snapshot";

pub const CONFIG_HISTORY_TOOL_NAMES: &[&str] =
    &[TOOL_LIST_CONFIG_CHANGES, TOOL_GET_CONFIG_SNAPSHOT];

pub fn config_history_tools() -> Vec<Tool> {
    vec![list_config_changes_tool(), get_config_snapshot_tool()]
}

pub fn owns(name: &str) -> bool {
    CONFIG_HISTORY_TOOL_NAMES.contains(&name)
}

fn string_property(description: &str) -> Value {
    json!({"type": "string", "description": description})
}

fn list_config_changes_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "subject_kind",
                string_property("setting, toggle, steerer, build or promotion."),
            ),
            (
                "subject_key",
                string_property(
                    "Exact key: a setting or toggle key, a steerer id, mike for builds, or a role for promotions.",
                ),
            ),
            (
                "subject_key_prefix",
                string_property("Keys starting with this text, for example judge_worker."),
            ),
            (
                "via",
                string_property("spa, api, startup, migration, promotion, mike or sql."),
            ),
            (
                "since",
                string_property("RFC 3339 lower bound on changed_at."),
            ),
            (
                "until",
                string_property("RFC 3339 upper bound on changed_at."),
            ),
            (
                "before_id",
                json!({"type": "integer", "description": "Keyset cursor: next_before_id from the previous page."}),
            ),
            (
                "limit",
                json!({"type": "integer", "description": "Rows per page, 1 to 500, default 100."}),
            ),
        ],
        &[],
    );
    Tool::new(
        Cow::Borrowed(TOOL_LIST_CONFIG_CHANGES),
        Cow::Borrowed(
            "Every change to mike's settings, feature toggles and steerers, plus builds and \
             promotions, newest first, with the complete old and new values, the surface that \
             made it (spa, api, startup, migration, promotion, mike, sql), the actor and the \
             build. A build row also carries that build's code defaults for every setting and \
             toggle, and the keys whose default changed. Page with before_id. Secret-shaped \
             keys show {\"redacted\": true}.",
        ),
        schema,
    )
}

fn get_config_snapshot_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "at",
                string_property("RFC 3339 moment to rebuild the configuration at."),
            ),
            (
                "subject_kind",
                string_property("Only setting, toggle or steerer entries."),
            ),
        ],
        &["at"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_GET_CONFIG_SNAPSHOT),
        Cow::Borrowed(
            "The value of every setting, toggle and steerer in force at a moment: the stored \
             value where a row existed (source: stored), otherwise the code default of the build \
             running then (source: code_default), or source: unknown before the first build row. \
             Nothing is known before `baseline_at`.",
        ),
        schema,
    )
}

#[derive(Debug, Default, Deserialize)]
struct ListConfigChangesArgs {
    subject_kind: Option<String>,
    subject_key: Option<String>,
    subject_key_prefix: Option<String>,
    via: Option<String>,
    since: Option<String>,
    until: Option<String>,
    before_id: Option<i64>,
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct GetConfigSnapshotArgs {
    at: String,
    subject_kind: Option<String>,
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    match name {
        TOOL_LIST_CONFIG_CHANGES => {
            let args = parse_args_default::<ListConfigChangesArgs>(arguments)?;
            let query = ConfigChangesQuery {
                subject_kind: args.subject_kind.as_deref(),
                subject_key: args.subject_key.as_deref(),
                subject_key_prefix: args.subject_key_prefix.as_deref(),
                via: args.via.as_deref(),
                since: args.since.as_deref(),
                until: args.until.as_deref(),
                before_id: args.before_id,
                limit: args.limit,
            };
            let value = client
                .list_config_changes(&query)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("change_log", value))
        }
        TOOL_GET_CONFIG_SNAPSHOT => {
            let args = parse_args_required::<GetConfigSnapshotArgs>(arguments)?;
            let value = client
                .get_config_snapshot(&args.at, args.subject_kind.as_deref())
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("snapshot", value))
        }
        other => Err(RmcpError::invalid_params(
            format!("unknown config history tool: {other}"),
            None,
        )),
    }
}
