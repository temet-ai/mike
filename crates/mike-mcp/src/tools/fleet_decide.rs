use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde_json::{Map, Value, json};

use super::{arguments_as_value, object_schema, wrap_success};
use crate::client::InspectClient;
use crate::error::McpError;

pub const TOOL_FLEET_DECIDE: &str = "fleet_decide";

pub const DECISIONS: &[&str] = &[
    "record_fault",
    "instruct_seats",
    "patch_definition",
    "new_task",
    "waive_obligation",
    "amend_ruling",
    "accept_substitute_proof",
    "disregard_receipt",
    "rerun_targeted",
    "change_setting",
    "escalate",
];

pub fn fleet_decide_tools() -> Vec<Tool> {
    vec![decide_tool()]
}

pub fn owns(name: &str) -> bool {
    name == TOOL_FLEET_DECIDE
}

fn text(description: &str) -> Value {
    json!({"type": "string", "description": description})
}

fn decide_tool() -> Tool {
    let schema = object_schema(
        vec![
            ("fleet", text("The fleet id or slug (get_fleet).")),
            (
                "session_id",
                text(
                    "Your own session id: the problem-solver session mike started for this problem.",
                ),
            ),
            (
                "problem_id",
                json!({"type": "string", "format": "uuid", "description": "The problem you were started for, from your brief."}),
            ),
            (
                "decision",
                json!({"type": "string", "enum": DECISIONS, "description": "The one action you decide on. `escalate` asks the operator one question."}),
            ),
            (
                "reason",
                text("Why this decision fits the problem, in a sentence or two."),
            ),
            (
                "evidence_read",
                json!({"type": "array", "minItems": 1, "items": {"type": "string", "minLength": 1}, "description": "What you read to decide: note ids, receipt ids, file paths, session ids. At least one."}),
            ),
            (
                "question",
                text(
                    "Required when `decision` is `escalate`: the one specific question for the operator, saying what each answer would set in motion.",
                ),
            ),
        ],
        &[
            "fleet",
            "session_id",
            "problem_id",
            "decision",
            "reason",
            "evidence_read",
        ],
    );
    Tool::new(
        Cow::Borrowed(TOOL_FLEET_DECIDE),
        Cow::Borrowed(
            "Record your decision on the one problem you were started for. Mutating; only the \
             problem-solver profile serves it. Give `problem_id`, one `decision` from the \
             allowed set, a `reason` and what you read in `evidence_read`; `escalate` also \
             needs `question`. A call missing one is refused naming the field. mike records \
             the decision as events and acts on it. Returns `{decision: {problem_id, \
             recorded, escalated}}`. `select` roots at `/decision`.",
        ),
        schema,
    )
}

fn missing(name: &str, hint: &str) -> RmcpError {
    RmcpError::invalid_params(
        format!("fleet_decide is incomplete: `{name}` is missing; {hint}"),
        None,
    )
}

fn text_field<'a>(body: &'a Value, name: &str) -> Option<&'a str> {
    body.get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

pub(crate) fn decision_problem(body: &Value) -> Option<RmcpError> {
    if text_field(body, "problem_id").is_none() {
        return Some(missing("problem_id", "give the problem id from your brief"));
    }
    let Some(decision) = text_field(body, "decision") else {
        return Some(missing(
            "decision",
            &format!("give one of {}", DECISIONS.join(", ")),
        ));
    };
    if !DECISIONS.contains(&decision) {
        return Some(RmcpError::invalid_params(
            format!(
                "fleet_decide: `decision` {decision:?} is not allowed; give one of {}",
                DECISIONS.join(", ")
            ),
            None,
        ));
    }
    if text_field(body, "reason").is_none() {
        return Some(missing("reason", "say why this decision fits the problem"));
    }
    let read_something = body
        .get("evidence_read")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items
                .iter()
                .any(|v| v.as_str().is_some_and(|s| !s.trim().is_empty()))
        });
    if !read_something {
        return Some(missing(
            "evidence_read",
            "list at least one thing you read to decide",
        ));
    }
    if decision == "escalate" && text_field(body, "question").is_none() {
        return Some(missing(
            "question",
            "an `escalate` decision needs the one question for the operator",
        ));
    }
    None
}

fn fleet_route(fleet: &str) -> Result<String, McpError> {
    let mut url = reqwest::Url::parse("http://x")
        .map_err(|e| McpError::Parse(format!("bad dummy base: {e}")))?;
    url.path_segments_mut()
        .map_err(|_| McpError::Parse("dummy url cannot be a base".to_string()))?
        .extend(["api", "fleets", fleet, "decide"]);
    Ok(url.path().to_string())
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    let mut body = arguments_as_value(arguments);
    let fleet = body
        .as_object_mut()
        .and_then(|fields| fields.remove("fleet"))
        .and_then(|v| v.as_str().map(str::to_string))
        .ok_or_else(|| missing("fleet", "give the fleet id or slug"))?;
    if text_field(&body, "session_id").is_none() {
        return Err(missing("session_id", "give your own session id"));
    }
    if let Some(problem) = decision_problem(&body) {
        return Err(problem);
    }
    let path = fleet_route(&fleet).map_err(McpError::into_rmcp)?;
    let value = client
        .post_json(&path, &body)
        .await
        .map_err(McpError::into_rmcp)?;
    Ok(wrap_success("decision", value))
}

#[cfg(test)]
#[path = "fleet_decide_tests.rs"]
mod tests;
