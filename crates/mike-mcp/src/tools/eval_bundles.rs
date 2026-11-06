use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{object_schema, parse_args_default, wrap_success};
use crate::client::{BundlesQuery, InspectClient};
use crate::error::McpError;

pub const TOOL_LIST_EVAL_BUNDLES: &str = "list_eval_bundles";

pub const EVAL_BUNDLE_TOOL_NAMES: &[&str] = &[TOOL_LIST_EVAL_BUNDLES];

pub fn eval_bundle_tools() -> Vec<Tool> {
    vec![list_eval_bundles_tool()]
}

pub fn owns(name: &str) -> bool {
    EVAL_BUNDLE_TOOL_NAMES.contains(&name)
}

fn text(description: &str) -> Value {
    json!({"type": "string", "description": description})
}

fn list_eval_bundles_tool() -> Tool {
    let schema = object_schema(
        vec![
            ("role", text("turn_judge, session_judge or steerer.")),
            (
                "subject_key",
                text("The steerer id for a steerer bundle; empty for the judges."),
            ),
            ("origin", text("resolver or candidate.")),
            (
                "status",
                text("candidate, shadow, live, live_ungated or retired."),
            ),
            (
                "current",
                json!({"type": "boolean", "description": "true: only the bundles running now."}),
            ),
            (
                "bundle_id",
                json!({"type": "integer", "description": "Return this one bundle."}),
            ),
            (
                "before_id",
                json!({"type": "integer", "description": "Keyset cursor: next_before_id from the previous page."}),
            ),
            (
                "limit",
                json!({"type": "integer", "description": "Rows per page, 1 to 200, default 50."}),
            ),
        ],
        &[],
    );
    Tool::new(
        Cow::Borrowed(TOOL_LIST_EVAL_BUNDLES),
        Cow::Borrowed(
            "List the configurations mike's judges and steerers run with (model, complete prompt, \
             output schema, input-builder fingerprints, thresholds), one row per version, newest \
             first. Stored turn outcomes, session verdicts, steerer evaluations and steerer \
             fires name a bundle by id (`outcome_bundle_id` or `bundle_id`).",
        ),
        schema,
    )
}

#[derive(Debug, Default, Deserialize)]
struct ListEvalBundlesArgs {
    role: Option<String>,
    subject_key: Option<String>,
    origin: Option<String>,
    status: Option<String>,
    current: Option<bool>,
    bundle_id: Option<i64>,
    before_id: Option<i64>,
    limit: Option<i64>,
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    if name != TOOL_LIST_EVAL_BUNDLES {
        return Err(RmcpError::invalid_params(
            format!("unknown eval bundle tool: {name}"),
            None,
        ));
    }
    let args = parse_args_default::<ListEvalBundlesArgs>(arguments)?;
    if let Some(id) = args.bundle_id {
        let value = client
            .get_eval_bundle(id)
            .await
            .map_err(McpError::into_rmcp)?;
        return Ok(wrap_success("bundle", value));
    }
    let query = BundlesQuery {
        role: args.role.as_deref(),
        subject_key: args.subject_key.as_deref(),
        origin: args.origin.as_deref(),
        status: args.status.as_deref(),
        current: args.current,
        before_id: args.before_id,
        limit: args.limit,
    };
    let value = client
        .list_eval_bundles(&query)
        .await
        .map_err(McpError::into_rmcp)?;
    Ok(wrap_success("bundles", value))
}
