use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{object_schema, parse_args_required, wrap_success};
use crate::client::InspectClient;
use crate::error::McpError;

pub const TOOL_GET_FLEET_RECORD: &str = "get_fleet_record";

#[derive(Debug, Deserialize)]
struct GetFleetRecordArgs {
    fleet: String,
    #[serde(default)]
    against: Option<String>,
    #[serde(default)]
    recompute: Option<bool>,
}

pub fn fleet_record_tools() -> Vec<Tool> {
    vec![get_fleet_record_tool()]
}

pub fn owns(name: &str) -> bool {
    name == TOOL_GET_FLEET_RECORD
}

fn get_fleet_record_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "fleet",
                json!({
                    "type": "string",
                    "description": "The fleet id or slug (get_fleet) of a run that has closed."
                }),
            ),
            (
                "against",
                json!({
                    "type": "string",
                    "description": "Another fleet's id or slug. The answer then also carries \
                        the two records side by side with the difference of each number."
                }),
            ),
            (
                "recompute",
                json!({
                    "type": "boolean",
                    "description": "true computes the record again from the capture database \
                        and stores it in place of the one written when the run closed. \
                        Default false: the stored record."
                }),
            ),
        ],
        &["fleet"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_GET_FLEET_RECORD),
        Cow::Borrowed(
            "Read the measured record of a fleet's run, to score it and compare it with \
             another run. Read-only unless `recompute` is true. The record holds the four \
             numbers: time to done, minutes lost to fleet faults (with the breakdown by \
             cause), tokens per accepted task by seat kind, and manual interventions; and the \
             counts that explain them (seats per task, relaunches, checks run and reused, \
             verdict latency, unread messages at close, tool errors per seat). With \
             `against` the answer is `{record: {record, against: {fleet, record, \
             difference}}}`: the run's record, the other run's record and each numeric field \
             as the run's value minus the other's. A fleet that has not closed has no record \
             yet and the call says so. A `select` pointer starts at the top of the answer, \
             e.g. `/record/time_to_done_minutes` or `/record/fault_minutes/total`; with \
             `against`, `/record/against/difference`.",
        ),
        schema,
    )
}

pub async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    match name {
        TOOL_GET_FLEET_RECORD => call_get_fleet_record(client, arguments).await,
        other => Err(RmcpError::invalid_params(
            format!("unknown fleet record tool: {other}"),
            None,
        )),
    }
}

fn record_query(args: &GetFleetRecordArgs) -> Vec<(&'static str, String)> {
    let against = args.against.clone().map(|against| ("against", against));
    let recompute = args
        .recompute
        .filter(|recompute| *recompute)
        .map(|_| ("recompute", "true".to_string()));
    against.into_iter().chain(recompute).collect()
}

async fn call_get_fleet_record(
    client: &InspectClient,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    let args = parse_args_required::<GetFleetRecordArgs>(arguments)?;
    let value = client
        .get_fleet_record(&args.fleet, &record_query(&args))
        .await
        .map_err(McpError::into_rmcp)?;
    Ok(wrap_success("record", value))
}

#[cfg(test)]
#[path = "fleet_record_tests.rs"]
mod fleet_record_tests;
