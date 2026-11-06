use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde_json::{Map, Value, json};

use super::{object_schema, wrap_success};
use crate::client::InspectClient;
use crate::error::McpError;

pub const TOOL_GET_IMPROVEMENT_EFFECTS: &str = "get_improvement_effects";

pub const IMPROVEMENT_TOOL_NAMES: &[&str] = &[TOOL_GET_IMPROVEMENT_EFFECTS];

pub fn owns(name: &str) -> bool {
    IMPROVEMENT_TOOL_NAMES.contains(&name)
}

pub fn improvement_tools() -> Vec<Tool> {
    vec![Tool::new(
        Cow::Borrowed(TOOL_GET_IMPROVEMENT_EFFECTS),
        Cow::Borrowed(
            "List every action the judge's labels drove (a lesson approved from a correction, an \
             approved directive, a turn-guard question) with its measured effect: a before/after \
             or holdback/treated share of corrected turns, a 95% Newcombe interval once the read \
             point is reached, or `not_enough_data` with the sample size below it. `project` \
             narrows lesson actions to one project; guard questions are global and appear only in \
             the unfiltered call. Lesson before/after uses curated labels, not live judge verdicts. \
             Set include_exposures for separate seven-day delivery/live-outcome associations; neither establishes causal benefit.",
        ),
        object_schema(
            vec![
                (
                    "include_exposures",
                    json!({"type":"boolean", "description":"Include seven-day recorded delivery and live-outcome associations."}),
                ),
                (
                    "project",
                    json!({"type": "string", "description": "Project path to narrow lesson actions to."}),
                ),
            ],
            &[],
        ),
    )]
}

fn pairs(
    arguments: Option<&Map<String, Value>>,
    keys: &[&'static str],
) -> Vec<(&'static str, String)> {
    let Some(a) = arguments else {
        return Vec::new();
    };
    keys.iter()
        .filter_map(|k| {
            let s = match a.get(*k)? {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                _ => return None,
            };
            Some((*k, s))
        })
        .collect()
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    let args = arguments.as_ref();
    let value = match name {
        TOOL_GET_IMPROVEMENT_EFFECTS => {
            client
                .improvement_get(
                    "/api/improvement/actions",
                    &pairs(args, &["project", "include_exposures"]),
                )
                .await
        }
        _ => {
            return Err(RmcpError::invalid_params(
                format!("unknown improvement tool: {name}"),
                None,
            ));
        }
    };
    Ok(wrap_success(
        "improvement",
        value.map_err(McpError::into_rmcp)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn improvement_tools_serve_every_name() {
        assert_eq!(improvement_tools().len(), IMPROVEMENT_TOOL_NAMES.len());
        assert!(owns(TOOL_GET_IMPROVEMENT_EFFECTS));
    }
}
