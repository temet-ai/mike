use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::fleets::TOOL_GET_FLEET;
use super::{object_schema, parse_args_required, wrap_success};
use crate::client::InspectClient;
use crate::error::McpError;

#[derive(Debug, Deserialize)]
struct GetFleetArgs {
    fleet: String,
    #[serde(default)]
    view: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
}

pub(super) fn get_fleet_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "fleet",
                json!({
                    "type": "string",
                    "description": "The fleet id or the slug returned by define_fleet."
                }),
            ),
            (
                "view",
                json!({
                    "type": "string",
                    "enum": ["seat", "digest", "full"],
                    "description": "How much comes back. `seat`: your own task and your peers by name, session and state, with no briefs. `digest`: task readiness and acceptance, fleet accepted status, full obligations, criterion progress, idle seconds, verdict counts, and builds waiting for a slot. `full` (the default): the whole definition."
                }),
            ),
            (
                "session_id",
                json!({
                    "type": "string",
                    "description": "Your own session id; the `seat` view finds your task by it."
                }),
            ),
        ],
        &["fleet"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_GET_FLEET),
        Cow::Borrowed(
            "Read one fleet. Read-only. A worker reads it with `view: \"seat\"` and its own \
             `session_id`: `{fleet: {you, peers}}`, where `you` is its task (name, discipline, \
             criteria, files, seat, session, state) and each peer is a task with its name, seat, \
             session and state, and no briefs. mike mails the control seats this digest whenever \
             the fleet changes; `view: \"digest\"` reads it on demand: `{fleet: {tasks: [{task, \
             seat, state, ready, accepted, idle_seconds, passes, fails}], accepted, criteria, obligations, waiting_for_slot, build_slots}}`. Without \
             `view` it returns `{fleet: {...}}` with `state` (draft, ready, starting, running, \
             failed or done), the whole `definition` with every brief, `missing` [{`item`, \
             `problem`}], `ready` (true when nothing is missing), `run_id` and timestamps, and, \
             until the fleet starts, `at_ready` {`starts`: the seats mike would start; \
             `machine`: what this machine would refuse}. A `select` pointer starts at `/fleet`, \
             e.g. `/fleet/state` or `/fleet/definition/tasks`.",
        ),
        schema,
    )
}

pub(super) async fn call_get_fleet(
    client: &InspectClient,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    let args = parse_args_required::<GetFleetArgs>(arguments)?;
    let query: Vec<(&str, String)> = [("view", args.view), ("session", args.session_id)]
        .into_iter()
        .filter_map(|(key, value)| Some((key, value?)))
        .collect();
    let value = client
        .get_fleet_view(&args.fleet, &query)
        .await
        .map_err(McpError::into_rmcp)?;
    Ok(wrap_success("fleet", value))
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::super::fleets::call_tool;
    use super::*;

    fn arguments(value: Value) -> Option<Map<String, Value>> {
        value.as_object().cloned()
    }

    #[tokio::test]
    async fn get_fleet_with_the_seat_view_asks_for_it_with_the_callers_session()
    -> anyhow::Result<()> {
        let server = MockServer::start().await;
        let client = InspectClient::new(server.uri());
        Mock::given(method("GET"))
            .and(path("/api/fleets/f1"))
            .and(query_param("view", "seat"))
            .and(query_param("session", "s-1"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"you": {"task": "T4"}, "peers": []})),
            )
            .mount(&server)
            .await;
        let result = call_tool(
            &client,
            TOOL_GET_FLEET,
            arguments(json!({"fleet": "f1", "view": "seat", "session_id": "s-1"})),
        )
        .await?;
        assert_eq!(
            result.structured_content,
            Some(json!({"fleet": {"you": {"task": "T4"}, "peers": []}}))
        );
        Ok(())
    }

    #[test]
    fn the_tool_names_the_views_and_takes_a_session_id() {
        let tool = get_fleet_tool();
        let properties = tool.input_schema.get("properties");
        for name in ["view", "session_id", "select"] {
            assert!(properties.and_then(|p| p.get(name)).is_some(), "{name}");
        }
    }
}
