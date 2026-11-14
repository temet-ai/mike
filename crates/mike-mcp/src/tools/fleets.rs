use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{object_schema, parse_args_required, wrap_success};
use crate::client::InspectClient;
use crate::error::McpError;

pub const FLEET_TOOL_NAMES: &[&str] = &[TOOL_DEFINE_FLEET, TOOL_GET_FLEET];

pub const TOOL_DEFINE_FLEET: &str = "define_fleet";
pub const TOOL_GET_FLEET: &str = "get_fleet";

pub fn fleet_tools() -> Vec<Tool> {
    vec![define_fleet_tool(), super::fleet_views::get_fleet_tool()]
}

pub fn owns(name: &str) -> bool {
    FLEET_TOOL_NAMES.contains(&name)
}

fn define_fleet_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "fleet",
                json!({
                    "type": "string",
                    "description": "The fleet to change: the `fleet` id or the slug an earlier \
                        call returned. Omit it to create a new fleet, which then needs `slug` \
                        and `project_path`."
                }),
            ),
            (
                "slug",
                json!({
                    "type": "string",
                    "description": "A short name for a new fleet, such as \"night-run\": \
                        letters, digits, `-`, `_` and `.` only. Used only when `fleet` is \
                        omitted."
                }),
            ),
            (
                "project_path",
                json!({
                    "type": "string",
                    "description": "The folder of the project the new fleet works for. Used \
                        only when `fleet` is omitted."
                }),
            ),
            (
                "created_by",
                json!({
                    "type": "string",
                    "description": "Your own session id. Filled in from your session when you \
                        create a fleet; mike mails it a failed start and a fleet that stops \
                        moving."
                }),
            ),
            (
                "patch",
                json!({
                    "type": "object",
                    "description": "The items to set, merged into what is stored. An item you \
                        name is set (objects such as criteria, workers and tasks merge entry by \
                        entry, so naming one task leaves the others alone); an item you leave \
                        out stays as it is; an item set to null is removed. A list, such as \
                        rules or a task's files, is replaced whole."
                }),
            ),
            (
                "ready",
                json!({
                    "type": "boolean",
                    "description": "true asks mike to run every check, including the ones \
                        against the machine (the repo folder, the agent files, the contract \
                        files, room in the terminal). The fleet becomes ready only when \
                        nothing is missing; you cannot declare it ready."
                }),
            ),
        ],
        &[],
    );
    Tool::new(
        Cow::Borrowed(TOOL_DEFINE_FLEET),
        Cow::Borrowed(
            "Create or change a fleet definition. Mutating. Create: `slug`, `project_path`, \
             `patch`. Change: `fleet`, `patch`; merges named items, null removes, lists replace. \
             Items: `goal` (one sentence: what is true when the fleet is done); `ask` (the \
             operator's words, verbatim); `repo` (full path); `branch`; `rules` (list: what no \
             seat may do in this run); `shared_files` (list: files no single task owns); `check` \
             {`command`, `paths`}; `reference` {`enabled`}; `briefs` {`as_notes`: opt-in session notes}; \
             `criteria.<id>` {`text`, `check`, `paths`: optional declared end-to-end repo paths, \
             caller through final implementation; each needs a task owner; no inference from prose}; \
             `coordinator`, `acceptance`, `observer` {`model`, `agent`, `brief`; the observer \
             also `contract`}; `workers.<discipline>` {`model`, `agent`, `contract`}; \
             `tasks.<id>` {`name`, `discipline`, `criteria`, `files`, `brief`, `after`, `goal`}; \
             `goals` {`workers`: a /goal condition for every worker, `{task}`, `{name}`, \
             `{criteria}` filled in; `tasks.<id>.goal` overrides it; control seats get none}. A fleet \
             has at least two tasks, and no two tasks share a file. Setting `tasks.<id>.seat` and \
             `tasks.<id>.session` to null has mike start a fresh seat for that task. A task's \
             state is worked out by mike from its seat, reports and verdicts: a patch that sets \
             `tasks.<id>.state` is saved and not shown. Returns \
             `state`, `definition`, `missing` [{`item`, `problem`}] and `at_ready` {`starts`: the \
             seats mike would start; `machine`: what this machine would refuse}. Fix what \
             `missing` and `at_ready.machine` list and call again. `ready: true` runs every \
             check; with nothing missing the fleet becomes ready and mike starts it. Never launch \
             its seats yourself. Errors: `invalid_definition` (an item has the wrong shape; \
             nothing was saved), `fleet_starting` (it cannot change while it launches), \
             `fleet_changed` (it changed while you saved; read it with get_fleet and send the \
             patch again).",
        ),
        schema,
    )
}

#[derive(Debug, Deserialize)]
pub(crate) struct DefineFleetArgs {
    #[serde(default)]
    pub(crate) fleet: Option<String>,
    #[serde(default)]
    pub(crate) slug: Option<String>,
    #[serde(default)]
    pub(crate) project_path: Option<String>,
    #[serde(default)]
    pub(crate) patch: Option<Value>,
    #[serde(default)]
    pub(crate) ready: Option<bool>,
    #[serde(default)]
    pub(crate) created_by: Option<String>,
}

fn create_body(
    slug: &str,
    project_path: &str,
    patch: Option<Value>,
    created_by: Option<&str>,
) -> Value {
    let mut body = Map::new();
    body.insert("slug".to_string(), Value::String(slug.to_string()));
    body.insert(
        "project_path".to_string(),
        Value::String(project_path.to_string()),
    );
    if let Some(author) = created_by.filter(|author| !author.trim().is_empty()) {
        body.insert(
            "created_by".to_string(),
            Value::String(author.trim().to_string()),
        );
    }
    if let Some(patch) = patch {
        body.insert("definition".to_string(), patch);
    }
    Value::Object(body)
}

fn patch_body(patch: Option<Value>, ready: Option<bool>) -> Value {
    let mut body = Map::new();
    if let Some(patch) = patch {
        body.insert("patch".to_string(), patch);
    }
    if let Some(ready) = ready {
        body.insert("ready".to_string(), Value::Bool(ready));
    }
    Value::Object(body)
}

fn needed(value: Option<String>, name: &str) -> Result<String, RmcpError> {
    value.filter(|v| !v.trim().is_empty()).ok_or_else(|| {
        RmcpError::invalid_params(
            format!(
                "`{name}` is required to create a fleet; to change an existing one pass `fleet`"
            ),
            None,
        )
    })
}

async fn create_then_ask_ready(
    client: &InspectClient,
    args: DefineFleetArgs,
) -> Result<Value, RmcpError> {
    let slug = needed(args.slug, "slug")?;
    let project_path = needed(args.project_path, "project_path")?;
    let created = client
        .create_fleet(&create_body(
            &slug,
            &project_path,
            args.patch,
            args.created_by.as_deref(),
        ))
        .await
        .map_err(McpError::into_rmcp)?;
    if args.ready != Some(true) {
        return Ok(created);
    }
    let id = created["fleet"].as_str().ok_or_else(|| {
        McpError::Parse(format!("the created fleet has no id: {created}")).into_rmcp()
    })?;
    client
        .patch_fleet(id, &patch_body(None, Some(true)))
        .await
        .map_err(|e| {
            RmcpError::internal_error(
                format!(
                    "the fleet was created as `{id}` but asking for ready failed: {e}. Call \
                     define_fleet with fleet=\"{id}\" and ready=true to ask again; do not create \
                     it a second time."
                ),
                None,
            )
        })
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    match name {
        TOOL_DEFINE_FLEET => {
            let args = parse_args_required::<DefineFleetArgs>(arguments)?;
            let value = match args.fleet.clone() {
                Some(fleet) => client
                    .patch_fleet(&fleet, &patch_body(args.patch, args.ready))
                    .await
                    .map_err(McpError::into_rmcp)?,
                None => create_then_ask_ready(client, args).await?,
            };
            Ok(wrap_success("fleet", value))
        }
        TOOL_GET_FLEET => super::fleet_views::call_get_fleet(client, arguments).await,
        other => Err(RmcpError::invalid_params(
            format!("unknown fleet tool: {other}"),
            None,
        )),
    }
}

#[cfg(test)]
#[path = "fleets_tests.rs"]
mod tests;
