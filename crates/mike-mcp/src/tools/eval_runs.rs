use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde_json::{Map, Value, json};

use super::{object_schema, wrap_success};
use crate::client::InspectClient;
use crate::error::McpError;

pub const TOOL_LIST_EVAL_RUNS: &str = "list_eval_runs";
pub const TOOL_GET_EVAL_RUN: &str = "get_eval_run";
pub const TOOL_LIST_EVAL_RUN_ITEMS: &str = "list_eval_run_items";
pub const TOOL_LIST_EVAL_GATES: &str = "list_eval_gates";
pub const TOOL_GET_EVAL_GATE: &str = "get_eval_gate";

pub const EVAL_RUN_TOOL_NAMES: &[&str] = &[
    TOOL_LIST_EVAL_RUNS,
    TOOL_GET_EVAL_RUN,
    TOOL_LIST_EVAL_RUN_ITEMS,
    TOOL_LIST_EVAL_GATES,
    TOOL_GET_EVAL_GATE,
];

const EVAL_NOTE: &str = "Eval verdicts are never the stored label. A promotion gate needs corrected \
     precision >= 0.85 and recall >= 0.65 on a holdout split.";

pub fn owns(name: &str) -> bool {
    EVAL_RUN_TOOL_NAMES.contains(&name)
}

fn integer(description: &str) -> Value {
    json!({"type": "integer", "description": description})
}

fn text(description: &str) -> Value {
    json!({"type": "string", "description": description})
}

fn paging() -> Vec<(&'static str, Value)> {
    vec![
        ("limit", integer("Rows per page, 1 to 500, default 50.")),
        ("offset", integer("Rows to skip.")),
    ]
}

fn tool(
    name: &'static str,
    description: String,
    props: Vec<(&'static str, Value)>,
    required: &[&str],
) -> Tool {
    Tool::new(
        Cow::Borrowed(name),
        Cow::Owned(description),
        object_schema(props, required),
    )
}

pub fn eval_run_tools() -> Vec<Tool> {
    let mut runs = vec![
        (
            "status",
            text("running, paused_budget, complete, failed or cancelled."),
        ),
        ("split", text("dev or holdout.")),
        ("purpose", text("measure, noise or ablation.")),
        ("role", text("Bundle role: turn_judge or session_judge.")),
    ];
    runs.extend(paging());
    let mut items = vec![
        ("run_id", integer("Eval run id.")),
        (
            "outcome",
            text("judged, refused, failed or skipped_unresolved."),
        ),
        (
            "correct",
            json!({"type": "boolean", "description": "Alias of reaction_correct, kept for older callers."}),
        ),
        (
            "reaction_correct",
            json!({"type": "boolean", "description": "Filter on reaction_correct."}),
        ),
        (
            "delivery_correct",
            json!({"type": "boolean", "description": "Filter on delivery_correct."}),
        ),
        (
            "disagreement",
            text("any, reaction or delivery: items the judge got wrong on that axis."),
        ),
        (
            "include",
            text(
                "Comma list. verdict: each item's whole turn verdict; review: severity, current gold and reviewed_at.",
            ),
        ),
    ];
    items.extend(paging());
    let mut gates = vec![("holdout_batch", text("One holdout batch."))];
    gates.extend(paging());
    vec![
        tool(
            TOOL_LIST_EVAL_RUNS,
            format!(
                "mike eval runs, newest first, whole rows with report and options. {EVAL_NOTE}"
            ),
            runs,
            &[],
        ),
        tool(
            TOOL_GET_EVAL_RUN,
            format!("One mike eval run, whole. {EVAL_NOTE}"),
            vec![("run_id", integer("Eval run id."))],
            &["run_id"],
        ),
        tool(
            TOOL_LIST_EVAL_RUN_ITEMS,
            format!(
                "The items of one eval run with gold labels, verdict ids and correctness. {EVAL_NOTE}"
            ),
            items,
            &["run_id"],
        ),
        tool(
            TOOL_LIST_EVAL_GATES,
            format!("mike eval gate decisions, newest first, with floors and report. {EVAL_NOTE}"),
            gates,
            &[],
        ),
        tool(
            TOOL_GET_EVAL_GATE,
            format!("One gate decision, whole. {EVAL_NOTE}"),
            vec![("gate_id", integer("Gate id."))],
            &["gate_id"],
        ),
    ]
}

fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn required_id(arguments: Option<&Map<String, Value>>, key: &str) -> Result<i64, RmcpError> {
    arguments
        .and_then(|a| a.get(key))
        .and_then(Value::as_i64)
        .ok_or_else(|| RmcpError::invalid_params(format!("{key} (integer) is required"), None))
}

fn query(arguments: Option<&Map<String, Value>>, skip: &str) -> Vec<(&'static str, String)> {
    const KEYS: [&str; 12] = [
        "status",
        "role",
        "split",
        "purpose",
        "outcome",
        "correct",
        "reaction_correct",
        "delivery_correct",
        "disagreement",
        "include",
        "holdout_batch",
        "limit",
    ];
    let Some(arguments) = arguments else {
        return Vec::new();
    };
    KEYS.iter()
        .chain(std::iter::once(&"offset"))
        .filter(|k| **k != skip)
        .filter_map(|k| Some((*k, scalar(arguments.get(*k)?)?)))
        .collect()
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    let args = arguments.as_ref();
    let value = match name {
        TOOL_LIST_EVAL_RUNS => {
            client
                .eval_runs_get("/api/eval/runs", &query(args, ""))
                .await
        }
        TOOL_GET_EVAL_RUN => {
            let id = required_id(args, "run_id")?;
            client
                .eval_runs_get_one(&format!("/api/eval/runs/{id}"), &format!("eval run {id}"))
                .await
        }
        TOOL_LIST_EVAL_RUN_ITEMS => {
            let id = required_id(args, "run_id")?;
            client
                .eval_runs_get(
                    &format!("/api/eval/runs/{id}/items"),
                    &query(args, "run_id"),
                )
                .await
        }
        TOOL_LIST_EVAL_GATES => {
            client
                .eval_runs_get("/api/eval/gates", &query(args, ""))
                .await
        }
        TOOL_GET_EVAL_GATE => {
            let id = required_id(args, "gate_id")?;
            client
                .eval_runs_get_one(&format!("/api/eval/gates/{id}"), &format!("eval gate {id}"))
                .await
        }
        _ => {
            return Err(RmcpError::invalid_params(
                format!("unknown eval run tool: {name}"),
                None,
            ));
        }
    };
    Ok(wrap_success("eval", value.map_err(McpError::into_rmcp)?))
}

#[cfg(test)]
mod tests {
    use super::{EVAL_RUN_TOOL_NAMES, eval_run_tools};

    #[test]
    fn every_eval_run_tool_is_served_and_names_the_floors() {
        let tools = eval_run_tools();
        assert_eq!(tools.len(), EVAL_RUN_TOOL_NAMES.len());
        for t in &tools {
            assert!(
                t.description
                    .as_deref()
                    .unwrap_or_default()
                    .contains("0.85")
            );
        }
    }
}
