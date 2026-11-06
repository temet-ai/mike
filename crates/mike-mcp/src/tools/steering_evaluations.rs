use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{object_schema, parse_args_required, wrap_success};
use crate::client::{InspectClient, SteererEvaluationsQuery};
use crate::error::McpError;

pub const TOOL_LIST_STEERER_EVALUATIONS: &str = "list_steerer_evaluations";

pub const STEERING_EVALUATION_TOOL_NAMES: &[&str] = &[TOOL_LIST_STEERER_EVALUATIONS];

pub fn steering_evaluation_tools() -> Vec<Tool> {
    vec![list_steerer_evaluations_tool()]
}

pub fn owns(name: &str) -> bool {
    STEERING_EVALUATION_TOOL_NAMES.contains(&name)
}

fn text(description: &str) -> Value {
    json!({"type": "string", "description": description})
}

fn integer(description: &str) -> Value {
    json!({"type": "integer", "description": description})
}

fn list_steerer_evaluations_tool() -> Tool {
    let schema = object_schema(
        vec![
            ("session", text("Session UUID.")),
            ("steerer", integer("Only this steerer id.")),
            (
                "question",
                text("relapse, unbacked_wait, authorised_handback or unobserved_done."),
            ),
            ("mode", text("live or shadow.")),
            (
                "status",
                text("dispatched, resolved_pass or resolved_correction."),
            ),
            ("limit", integer("Rows per page, 1 to 200, default 50.")),
            (
                "before_id",
                integer("Keyset cursor: next_before_id from the previous page."),
            ),
        ],
        &["session"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_LIST_STEERER_EVALUATIONS),
        Cow::Borrowed(
            "Every recorded steerer evaluation of one session, newest first, whole rows: \
             steerer, turn, status, mode, question, trigger, bundle, the stored verdict_json and \
             reason, and any outcome label. A shadow row's verdict_json is what the steerer would \
             have said; shadow rows never delivered anything.",
        ),
        schema,
    )
}

#[derive(Debug, Deserialize)]
struct ListSteererEvaluationsArgs {
    session: String,
    steerer: Option<i64>,
    question: Option<String>,
    mode: Option<String>,
    status: Option<String>,
    limit: Option<i64>,
    before_id: Option<i64>,
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    if name != TOOL_LIST_STEERER_EVALUATIONS {
        return Err(RmcpError::invalid_params(
            format!("unknown steering evaluation tool: {name}"),
            None,
        ));
    }
    let args = parse_args_required::<ListSteererEvaluationsArgs>(arguments)?;
    let query = SteererEvaluationsQuery {
        steerer_id: args.steerer,
        question: args.question.as_deref(),
        mode: args.mode.as_deref(),
        status: args.status.as_deref(),
        limit: args.limit,
        before_id: args.before_id,
    };
    let value = client
        .list_steerer_evaluations(&args.session, &query)
        .await
        .map_err(McpError::into_rmcp)?;
    Ok(wrap_success("evaluations", value))
}

#[cfg(test)]
mod tests {
    use super::{TOOL_LIST_STEERER_EVALUATIONS, steering_evaluation_tools};

    #[test]
    fn list_steerer_evaluations_requires_a_session() {
        let tools = steering_evaluation_tools();
        let tool = tools
            .iter()
            .find(|t| t.name == TOOL_LIST_STEERER_EVALUATIONS);
        let required = tool.and_then(|t| t.input_schema.get("required").cloned());
        assert_eq!(required, Some(serde_json::json!(["session"])));
    }
}
