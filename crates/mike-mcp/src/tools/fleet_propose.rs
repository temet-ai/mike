use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde_json::{Map, Value, json};

use super::{arguments_as_value, object_schema, wrap_success};
use crate::client::InspectClient;
use crate::error::McpError;

pub const TOOL_FLEET_PROPOSE: &str = "fleet_propose";
pub const TOOL_FLEET_SPLIT: &str = "fleet_split";
pub const FLEET_PROPOSE_TOOL_NAMES: &[&str] = &[TOOL_FLEET_PROPOSE, TOOL_FLEET_SPLIT];

pub fn fleet_propose_tools() -> Vec<Tool> {
    vec![propose_tool(), split_tool()]
}

pub fn owns(name: &str) -> bool {
    FLEET_PROPOSE_TOOL_NAMES.contains(&name)
}

fn strings(description: &str) -> Value {
    json!({"type": "array", "items": {"type": "string"}, "description": description})
}

fn text(description: &str) -> Value {
    json!({"type": "string", "description": description})
}

fn propose_tool() -> Tool {
    let schema = object_schema(
        vec![
            ("fleet", text("The fleet id or slug (get_fleet).")),
            (
                "session",
                text("Your own session id: the seat that proposes."),
            ),
            ("name", text("A short name for the new task.")),
            (
                "why",
                text("What the task does and why the run needs it; it opens the new seat's brief."),
            ),
            (
                "files",
                strings(
                    "Files or folders, relative to the repository, that the task may touch and that exist now.",
                ),
            ),
            ("new_files", strings("Files the task will create.")),
            (
                "criteria",
                strings("Ids of the fleet's criteria the task serves."),
            ),
            (
                "after",
                strings("Ids of tasks that must be ready before this one starts. Default none."),
            ),
            (
                "discipline",
                text("The discipline of the worker that takes it; one the fleet has workers for."),
            ),
        ],
        &[
            "fleet",
            "session",
            "name",
            "why",
            "files",
            "criteria",
            "discipline",
        ],
    );
    Tool::new(
        Cow::Borrowed(TOOL_FLEET_PROPOSE),
        Cow::Borrowed(
            "Propose a new task for the running fleet, for work you found that no task holds. \
             Mutating. mike checks it by rule, not by judgement: every file in `files` exists \
             (a file to be created goes in `new_files`), each criterion is one of the fleet's, \
             the discipline has workers, each `after` task exists, and no open task owns any \
             of the files. A clean proposal is added as a task at once, with a brief made from \
             `name`, `why` and the files, and mike starts a seat for it on its next pass; \
             nobody approves it. A proposal naming files another task owns is held, not \
             refused: mike mails that task's seat, which either releases the files (and mike \
             then adds the task) or declines. A broken rule is refused with each problem \
             named, so fix the call and send it again. Returns `{proposal: {proposal, state, \
             task, owners, problems}}`: `state` is `added` (with the new `task` id) or `held` \
             (with the owning tasks in `owners`). Any seat may call it, the coordinator and \
             acceptance included. `select` roots at `/proposal`.",
        ),
        schema,
    )
}

fn split_tool() -> Tool {
    let schema = object_schema(
        vec![
            ("fleet", text("The fleet id or slug (get_fleet).")),
            (
                "session",
                text("Your own session id: the seat that holds the task."),
            ),
            ("task", text("The id of your own task.")),
            ("name", text("A short name for the new task.")),
            (
                "why",
                text("What the split-off part is; it opens the new seat's brief."),
            ),
            (
                "files",
                strings(
                    "Entries of your task's file list, exactly as listed, that move to the new task.",
                ),
            ),
            (
                "criteria",
                strings(
                    "Criteria of your task that move with them. Default all of them; a criterion your task would be left without stays with it too.",
                ),
            ),
        ],
        &["fleet", "session", "task", "name", "files"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_FLEET_SPLIT),
        Cow::Borrowed(
            "Split part of your own task off into a new task, when it is more than one seat \
             should do. Mutating. Moves the named entries of your task's file list (and the \
             criteria named) to a new task that comes `after` yours, so mike starts a seat for \
             it once yours is recorded ready; your seat keeps the rest. Refused, naming the \
             problem, when the task is not held by `session`, a file is not an entry of its \
             list exactly as written, a criterion is not one of the task's, or your task would \
             keep no file. Returns `{split: {task, new_task, after, files, criteria, \
             kept_files, kept_criteria}}`. `select` roots at `/split`.",
        ),
        schema,
    )
}

fn fleet_route(fleet: &str, action: &str) -> Result<String, McpError> {
    let mut url = reqwest::Url::parse("http://x")
        .map_err(|e| McpError::Parse(format!("bad dummy base: {e}")))?;
    url.path_segments_mut()
        .map_err(|_| McpError::Parse("dummy url cannot be a base".to_string()))?
        .extend(["api", "fleets", fleet, action]);
    Ok(url.path().to_string())
}

fn split_body(arguments: Option<Map<String, Value>>) -> Result<(String, Value), RmcpError> {
    let mut body = arguments_as_value(arguments);
    let fleet = body
        .as_object_mut()
        .and_then(|fields| fields.remove("fleet"))
        .and_then(|v| v.as_str().map(str::to_string))
        .ok_or_else(|| {
            RmcpError::invalid_params("`fleet` is required: the fleet id or slug", None)
        })?;
    Ok((fleet, body))
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    let (action, key) = match name {
        TOOL_FLEET_PROPOSE => ("propose", "proposal"),
        TOOL_FLEET_SPLIT => ("split", "split"),
        other => {
            return Err(RmcpError::invalid_params(
                format!("unknown fleet tool: {other}"),
                None,
            ));
        }
    };
    let (fleet, body) = split_body(arguments)?;
    let path = fleet_route(&fleet, action).map_err(McpError::into_rmcp)?;
    let value = client
        .post_json(&path, &body)
        .await
        .map_err(McpError::into_rmcp)?;
    Ok(wrap_success(key, value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_tools_are_named_mutating_and_under_the_description_cap() {
        let tools = fleet_propose_tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        assert_eq!(names, FLEET_PROPOSE_TOOL_NAMES);
        for tool in &tools {
            let text = tool.description.as_deref().unwrap_or_default();
            assert!(text.contains("Mutating."), "{}", tool.name);
            assert!(text.len() <= 2048, "{} is {} chars", tool.name, text.len());
        }
        assert!(owns(TOOL_FLEET_SPLIT) && !owns("fleet_commit"));
    }

    #[test]
    fn the_fleet_goes_in_the_path_and_the_rest_in_the_body() {
        let mut args = Map::new();
        args.insert("fleet".into(), json!("change record/3"));
        args.insert("name".into(), json!("x"));
        let Ok((fleet, body)) = split_body(Some(args)) else {
            panic!("a fleet was given");
        };
        assert_eq!(body, json!({"name": "x"}));
        assert_eq!(
            fleet_route(&fleet, "propose").ok().as_deref(),
            Some("/api/fleets/change%20record%2F3/propose")
        );
        assert!(split_body(None).is_err());
    }
}
