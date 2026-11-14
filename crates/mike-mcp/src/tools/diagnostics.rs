use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{object_schema, order_property, parse_args_default, wrap_success};
use crate::client::InspectClient;
use crate::error::McpError;

pub const DIAGNOSTICS_TOOL_NAMES: &[&str] = &[
    TOOL_DIAGNOSTIC_STATS,
    TOOL_DIAGNOSTIC_FINDINGS,
    TOOL_DIAGNOSTIC_MEMORY,
    TOOL_GET_STATS,
    TOOL_GET_VERIFIER_RATE,
];

pub const TOOL_DIAGNOSTIC_STATS: &str = "diagnostic_stats";
pub const TOOL_DIAGNOSTIC_FINDINGS: &str = "diagnostic_findings";
pub const TOOL_DIAGNOSTIC_MEMORY: &str = "diagnostic_memory";
pub const TOOL_GET_STATS: &str = "get_stats";
pub const TOOL_GET_VERIFIER_RATE: &str = "get_verifier_rate";

pub fn diagnostics_tools() -> Vec<Tool> {
    vec![
        diagnostic_stats_tool(),
        diagnostic_findings_tool(),
        diagnostic_memory_tool(),
        get_stats_tool(),
        get_verifier_rate_tool(),
    ]
}

pub fn owns(name: &str) -> bool {
    DIAGNOSTICS_TOOL_NAMES.contains(&name)
}

fn diagnostic_stats_tool() -> Tool {
    let schema = object_schema(vec![("project", diagnostics_project_property())], &[]);
    Tool::new(
        Cow::Borrowed(TOOL_DIAGNOSTIC_STATS),
        Cow::Borrowed(
            "Aggregated diagnostic counts: totals by category, totals by analyzer and the last \
             24 hours.",
        ),
        schema,
    )
}

fn diagnostic_findings_tool() -> Tool {
    let schema = object_schema(
        vec![
            ("order", order_property()),
            ("project", diagnostics_project_property()),
        ],
        &[],
    );
    Tool::new(
        Cow::Borrowed(TOOL_DIAGNOSTIC_FINDINGS),
        Cow::Borrowed(
            "List individual diagnostic findings, newest first by default. Each finding: \
             `request_id`, `analyzer`, `category`, `matched_text`, `advice`.",
        ),
        schema,
    )
}

fn diagnostic_memory_tool() -> Tool {
    let schema = object_schema(
        vec![(
            "top",
            json!({
                "type": "integer",
                "minimum": 1,
                "maximum": 100,
                "description": "Number of largest entries to include in `top_entries` (default 10, max 100)."
            }),
        )],
        &[],
    );
    Tool::new(
        Cow::Borrowed(TOOL_DIAGNOSTIC_MEMORY),
        Cow::Borrowed(
            "Memory snapshot of the proxy's in-memory RequestStore: per-field byte sums, total \
             bytes, index key counts, in-flight count and peak RSS. `top_entries` lists the N \
             largest entries by total bytes, as ids and byte counts without content.",
        ),
        schema,
    )
}

fn get_stats_tool() -> Tool {
    let schema = object_schema(vec![], &[]);
    Tool::new(
        Cow::Borrowed(TOOL_GET_STATS),
        Cow::Borrowed(
            "Combined runtime, diagnostic and health snapshot, plus `liveness`: the daily \
             report of features wired but doing nothing (stage never applies, fleet event \
             never written, toggle off for weeks, shadow never promoted, job not succeeding, \
             unaudited fleet, memory verdict coverage), each finding with its figure and age. \
             Select `/liveness/findings` for the findings alone.",
        ),
        schema,
    )
}

fn get_verifier_rate_tool() -> Tool {
    let schema = object_schema(
        vec![(
            "project",
            json!({
                "type": "string",
                "description": "Substring match (case-insensitive) against the project path. \
                                Absent returns every project."
            }),
        )],
        &[],
    );
    Tool::new(
        Cow::Borrowed(TOOL_GET_VERIFIER_RATE),
        Cow::Borrowed(
            "Per-project fire counts of the Verifier steerer and the corrected rate \
             (`corrected_count` / `fires_count`). A fire is corrected when the user continued, \
             which marks it a true positive; otherwise it is a false alarm.",
        ),
        schema,
    )
}

fn diagnostics_project_property() -> Value {
    json!({
        "type": "string",
        "description": "Substring match (case-insensitive) against the project path."
    })
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct DiagnosticStatsArgs {
    #[serde(default)]
    pub(crate) project: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct OrderOnlyArgs {
    #[serde(default)]
    pub(crate) order: Option<String>,
    #[serde(default)]
    pub(crate) project: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct DiagnosticMemoryArgs {
    #[serde(default)]
    pub(crate) top: Option<u32>,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct GetVerifierRateArgs {
    #[serde(default)]
    pub(crate) project: Option<String>,
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    match name {
        TOOL_DIAGNOSTIC_STATS => {
            let args = parse_args_default::<DiagnosticStatsArgs>(arguments)?;
            let value = client
                .diagnostic_stats(args.project.as_deref())
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("stats", value))
        }
        TOOL_DIAGNOSTIC_FINDINGS => {
            let args = parse_args_default::<OrderOnlyArgs>(arguments)?;
            let value = client
                .diagnostic_findings(args.order.as_deref(), args.project.as_deref())
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("findings", value))
        }
        TOOL_DIAGNOSTIC_MEMORY => {
            let args = parse_args_default::<DiagnosticMemoryArgs>(arguments)?;
            let value = client
                .diagnostic_memory(args.top)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("memory", value))
        }
        TOOL_GET_STATS => {
            let value = client.combined_stats().await.map_err(McpError::into_rmcp)?;
            Ok(wrap_success("stats", value))
        }
        TOOL_GET_VERIFIER_RATE => {
            let args = parse_args_default::<GetVerifierRateArgs>(arguments)?;
            let value = client
                .get_verifier_rate(args.project.as_deref())
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("verifier_rate", value))
        }
        other => Err(RmcpError::invalid_params(
            format!("unknown diagnostics tool: {other}"),
            None,
        )),
    }
}
