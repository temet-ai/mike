use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde_json::{Map, Value, json};

use super::{object_schema, wrap_success};
use crate::client::InspectClient;
use crate::error::McpError;

pub const TOOL_WRIGHT_CHECKS: &str = "wright_checks";

pub const WRIGHT_CHECKS_TOOL_NAMES: &[&str] = &[TOOL_WRIGHT_CHECKS];

const ACTIONS: [&str; 4] = ["list", "add", "update", "remove"];

pub fn owns(name: &str) -> bool {
    WRIGHT_CHECKS_TOOL_NAMES.contains(&name)
}

pub fn wright_checks_tools() -> Vec<Tool> {
    vec![Tool::new(
        Cow::Borrowed(TOOL_WRIGHT_CHECKS),
        Cow::Borrowed(DESCRIPTION),
        object_schema(properties(), &["action"]),
    )]
}

const DESCRIPTION: &str = "List, add, change and remove Wright's checks: the stored rows the proxy runs on a schedule \
to find what goes wrong in mike itself (log lines, failed requests, tool and hook errors, worker runs, \
undelivered mail). A check has a `name`, `source`, `cadence` (event, regular, daily), `expects` (what a \
healthy run looks like), `probe` (sql, logs or cold_forks), `query` (sql probes only), `params` (its \
thresholds and windows) and `enabled`. A disabled check keeps its history; a removed one loses its runs \
and problems. `action` list returns every check, disabled ones too, with its latest run. `action` add \
needs name, source, cadence, expects, probe, and query for a sql probe. `action` update takes `name` and \
any of source, cadence, expects, query, params (merged into the stored params; a null value removes a key) and enabled. `action` remove \
takes `name`. A sql `query` is one read that returns (scope text, resource text, evidence jsonb) rows and \
reads its thresholds from `$1`, the params as jsonb, for example `WHERE failures >= ($1->>'min_failures')::int`. \
Add and update run it once in a read-only transaction; a 400 carries the database's error and writes nothing. \
A logs probe needs params {level: ERROR|WARN|INFO, repeats, window_minutes}; a cold_forks probe needs \
{window_hours, cache_ttl_minutes}. List first and copy a similar check's query and params.";

fn properties() -> Vec<(&'static str, Value)> {
    vec![
        (
            "action",
            json!({"type": "string", "enum": ACTIONS, "description": "What to do."}),
        ),
        (
            "name",
            json!({"type": "string", "description": "Check name: a-z, 0-9 and _, starting with a letter, 3 to 63 characters. Required for add, update and remove."}),
        ),
        (
            "source",
            json!({"type": "string", "enum": ["logs", "capture_db", "mcp_tools", "page", "hooks", "workers", "sessions"], "description": "Which signal the check reads."}),
        ),
        (
            "cadence",
            json!({"type": "string", "enum": ["event", "regular", "daily"], "description": "event: on a failed request and every pass; regular: every 5 minutes; daily: once a UTC day."}),
        ),
        (
            "expects",
            json!({"type": "string", "description": "One sentence: what a healthy run looks like."}),
        ),
        (
            "probe",
            json!({"type": "string", "enum": ["sql", "logs", "cold_forks"], "description": "add only; a probe cannot be changed afterwards."}),
        ),
        (
            "query",
            json!({"type": "string", "description": "sql probes only: the read that returns (scope, resource, evidence) rows and takes its thresholds from $1."}),
        ),
        (
            "params",
            json!({"type": "object", "description": "Thresholds and windows bound as $1; on update it replaces the stored object."}),
        ),
        (
            "enabled",
            json!({"type": "boolean", "description": "false stops the worker running the check; default true."}),
        ),
    ]
}

fn invalid(message: String) -> RmcpError {
    RmcpError::invalid_params(message, None)
}

fn take_name(args: &mut Map<String, Value>, action: &str) -> Result<String, RmcpError> {
    let name = args
        .remove("name")
        .and_then(|v| v.as_str().map(str::to_owned))
        .ok_or_else(|| {
            invalid(format!(
                "wright_checks {action} needs `name`, a check name as a string; arguments were {}",
                Value::Object(args.clone())
            ))
        })?;
    let well_formed = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if !well_formed {
        return Err(invalid(format!(
            "`name` is {name:?}; it may hold only a-z, 0-9 and _"
        )));
    }
    Ok(name)
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    if name != TOOL_WRIGHT_CHECKS {
        return Err(invalid(format!("unknown wright tool: {name}")));
    }
    let mut args = arguments.unwrap_or_default();
    let action = args
        .remove("action")
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    let value = match action.as_str() {
        "list" => client.wright_checks_list().await,
        "add" => client.wright_checks_add(Value::Object(args)).await,
        "update" => {
            let check = take_name(&mut args, "update")?;
            client
                .wright_checks_update(&check, Value::Object(args))
                .await
        }
        "remove" => {
            let check = take_name(&mut args, "remove")?;
            client.wright_checks_remove(&check).await
        }
        other => {
            return Err(invalid(format!(
                "`action` is {other:?}; it must be one of {}",
                ACTIONS.join(", ")
            )));
        }
    }
    .map_err(McpError::into_rmcp)?;
    Ok(wrap_success("wright", value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_description_fits_claude_codes_limit() {
        assert!(DESCRIPTION.len() < 2048, "{}", DESCRIPTION.len());
    }

    #[test]
    fn wright_checks_tools_serve_every_name() {
        assert_eq!(wright_checks_tools().len(), WRIGHT_CHECKS_TOOL_NAMES.len());
        assert!(owns(TOOL_WRIGHT_CHECKS));
    }

    #[test]
    fn take_name_refuses_a_missing_or_path_like_name() {
        let mut none = Map::new();
        assert!(take_name(&mut none, "update").is_err_and(|e| e.message.contains("needs `name`")));
        let mut path = Map::new();
        path.insert("name".into(), json!("../x"));
        assert!(take_name(&mut path, "remove").is_err_and(|e| e.message.contains("../x")));
    }
}
