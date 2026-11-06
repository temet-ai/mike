use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde_json::{Map, Value, json};

use super::{object_schema, wrap_success};
use crate::client::InspectClient;
use crate::error::McpError;

pub const TOOL_GET_EVAL_RATES: &str = "get_eval_rates";
pub const TOOL_LIST_EVAL_ALARMS: &str = "list_eval_alarms";

pub const EVAL_METRICS_TOOL_NAMES: &[&str] = &[TOOL_GET_EVAL_RATES, TOOL_LIST_EVAL_ALARMS];

const RATE_TEXT_FILTERS: &[(&str, &str)] = &[
    (
        "family",
        "accuracy, label_mix, base_rate or steer_calibration.",
    ),
    (
        "metric",
        "precision, recall, share, fire_rate, fire_precision or fire_recall.",
    ),
    (
        "population",
        "real_turns, replied_real_turns, real_session_ends, steerer_evaluations or labelled_turns.",
    ),
    ("role", "turn_judge, session_judge or steerer."),
    (
        "bundle_label",
        "bundle:<id>, or legacy:<model>:<rubric> for verdicts written before bundles.",
    ),
    (
        "axis",
        "The production axis (legacy_outcome, legacy_session_outcome) or label axis (reaction, session_end, steer_valid).",
    ),
    ("class", "The class scored, for example corrected."),
    (
        "label_tier",
        "gold = the operator's SPA label; relayed = an agent relaying the operator; seed = imported fleet-mirror data (dev); auto_shadow = next-message labels on shadow verdicts. Tiers are never pooled.",
    ),
    ("label_split", "dev or holdout."),
    (
        "label_source",
        "For example fleet_mirror/census, fleet_mirror/ground_truth, spa, mcp_relay.",
    ),
    ("scope", "global or project."),
    ("project", "Case-insensitive substring of the project path."),
    ("question", "The steerer question."),
    (
        "dimension",
        "writer_role, ended_on, confidence_bin, confidence_bin_05 or failure_tag.",
    ),
    ("from_day", "YYYY-MM-DD; first rollup day of a trend."),
    ("to_day", "YYYY-MM-DD; last rollup day of a trend."),
];

const RATE_INT_FILTERS: &[(&str, &str)] = &[
    ("bundle_id", "eval.bundles id."),
    ("window_days", "Window length; 0 selects all-history rows."),
    ("steerer_id", "Steerer id."),
    ("rollup_id", "One rollup."),
    ("limit", "Rows per page, 1 to 1,000, default 200."),
    ("offset", "Rows to skip."),
];

pub fn eval_metrics_tools() -> Vec<Tool> {
    vec![get_eval_rates_tool(), list_eval_alarms_tool()]
}

pub fn owns(name: &str) -> bool {
    EVAL_METRICS_TOOL_NAMES.contains(&name)
}

fn rate_properties() -> Vec<(&'static str, Value)> {
    let mut props: Vec<(&'static str, Value)> = RATE_TEXT_FILTERS
        .iter()
        .map(|(k, d)| (*k, json!({"type": "string", "description": d})))
        .collect();
    props.extend(
        RATE_INT_FILTERS
            .iter()
            .map(|(k, d)| (*k, json!({"type": "integer", "description": d}))),
    );
    props.push((
        "latest",
        json!({"type": "boolean", "description": "Only the newest committed rollup (default true unless a day range or rollup is given)."}),
    ));
    props
}

fn get_eval_rates_tool() -> Tool {
    Tool::new(
        Cow::Borrowed(TOOL_GET_EVAL_RATES),
        Cow::Borrowed(
            "Daily judgement metrics from the eval rollup: each judge's precision and recall against \
             the labels (per tier, split and source, never pooled), the stored label mix, base \
             rates and steerer calibration. Every row is k of n over a stated population and \
             window, with its Wilson interval and full detail.",
        ),
        object_schema(rate_properties(), &[]),
    )
}

fn list_eval_alarms_tool() -> Tool {
    Tool::new(
        Cow::Borrowed(TOOL_LIST_EVAL_ALARMS),
        Cow::Borrowed(
            "Drift and liveness alarm episodes (label_mix_shift, unjudged_backlog, judge_stalled, \
             budget_refusals, change_without_gate, gold_label_by_agent), open first, with their full \
             detail. Acknowledging is the operator's, in the SPA.",
        ),
        object_schema(
            vec![
                (
                    "state",
                    json!({"type": "string", "description": "open (default), cleared or all."}),
                ),
                (
                    "kind",
                    json!({"type": "string", "description": "One alarm kind."}),
                ),
                (
                    "acknowledged",
                    json!({"type": "boolean", "description": "Filter on acknowledgement."}),
                ),
                (
                    "since",
                    json!({"type": "string", "description": "RFC 3339; first seen at or after."}),
                ),
                (
                    "limit",
                    json!({"type": "integer", "description": "Rows per page, 1 to 1,000, default 100."}),
                ),
                (
                    "offset",
                    json!({"type": "integer", "description": "Rows to skip."}),
                ),
            ],
            &[],
        ),
    )
}

fn scalar_text(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn pairs(
    arguments: Option<&Map<String, Value>>,
    allowed: &[&'static str],
) -> Result<Vec<(&'static str, String)>, RmcpError> {
    let Some(arguments) = arguments else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for (key, value) in arguments {
        let Some(name) = allowed.iter().find(|a| **a == key.as_str()) else {
            return Err(RmcpError::invalid_params(
                format!("unknown argument {key:?}"),
                None,
            ));
        };
        let text = scalar_text(value).ok_or_else(|| {
            RmcpError::invalid_params(
                format!("argument {key:?} must be a scalar, got {value}"),
                None,
            )
        })?;
        out.push((*name, text));
    }
    Ok(out)
}

fn rate_keys() -> Vec<&'static str> {
    RATE_TEXT_FILTERS
        .iter()
        .chain(RATE_INT_FILTERS.iter())
        .map(|(k, _)| *k)
        .chain(std::iter::once("latest"))
        .collect()
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    match name {
        TOOL_GET_EVAL_RATES => {
            let query = pairs(arguments.as_ref(), &rate_keys())?;
            let value = client
                .get_eval_rates(&query)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("eval_rates", value))
        }
        TOOL_LIST_EVAL_ALARMS => {
            let allowed = ["state", "kind", "acknowledged", "since", "limit", "offset"];
            let query = pairs(arguments.as_ref(), &allowed)?;
            let value = client
                .list_eval_alarms(&query)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("eval_alarms", value))
        }
        _ => Err(RmcpError::invalid_params(
            format!("unknown eval metrics tool: {name}"),
            None,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{TOOL_GET_EVAL_RATES, eval_metrics_tools, rate_keys};

    #[test]
    fn get_eval_rates_schema_lists_every_rest_filter() {
        let tools = eval_metrics_tools();
        let rates = tools.iter().find(|t| t.name == TOOL_GET_EVAL_RATES);
        let props = rates
            .and_then(|t| t.input_schema.get("properties").cloned())
            .unwrap_or_default();
        for key in rate_keys() {
            assert!(props.get(key).is_some(), "{key} missing");
        }
    }

    #[test]
    fn eval_metrics_tools_absent_from_core_profile() {
        for name in super::EVAL_METRICS_TOOL_NAMES {
            assert!(!crate::tools::CORE_TOOL_NAMES.contains(name));
        }
    }
}
