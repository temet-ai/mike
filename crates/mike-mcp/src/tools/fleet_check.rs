use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{object_schema, parse_args_required, wrap_success};
use crate::client::InspectClient;
use crate::error::McpError;

pub const TOOL_FLEET_CHECK: &str = "fleet_check";
pub const TOOL_FLEET_WHERE: &str = "fleet_where";
const MAX_WAIT_SECONDS: u64 = 90;
pub const FLEET_CHECK_TOOL_NAMES: &[&str] = &[TOOL_FLEET_CHECK, TOOL_FLEET_WHERE];

pub fn fleet_check_tools() -> Vec<Tool> {
    vec![check_tool(), where_tool()]
}

pub fn owns(name: &str) -> bool {
    FLEET_CHECK_TOOL_NAMES.contains(&name)
}

fn fleet_property() -> (&'static str, Value) {
    (
        "fleet",
        json!({"type": "string", "description": "The fleet id or slug (get_fleet)."}),
    )
}

fn check_tool() -> Tool {
    let schema = object_schema(
        vec![
            fleet_property(),
            (
                "session_id",
                json!({"type": "string", "description": "Your own session id, so the answer is sorted into your files and other seats' files."}),
            ),
            (
                "status",
                json!({"type": "boolean", "description": "true only says whether the check has run at this state of the tree or is running now, and starts nothing. Default false."}),
            ),
            (
                "wait_seconds",
                json!({"type": "integer", "minimum": 0, "maximum": 90, "description": "Optional wait for completion, capped at 90 seconds. Default 0: return a durable queued receipt immediately; completion arrives by mail."}),
            ),
            (
                "command",
                json!({"type": "string", "description": "Exact project operation to queue on the shared checkout. Matches execution.operations for purpose, resources and environment. Exclusive with test/ruling; command receipts do not replace acceptance proof."}),
            ),
            (
                "test",
                json!({"type": "string", "description": "One test to run on its own, named as the project's focused-test command expects (definition `check.test_command`), for example `mike::cli claude_wrapper::x`. Runs only that test on the shared checkout (clean historical checkout with commit); the result is recorded for every seat."}),
            ),
            (
                "commit",
                json!({"type": "string", "description": "Run this exact historical commit (including HEAD) in mike's clean reusable checkout. Omitted: check the shared tree."}),
            ),
            (
                "ruling",
                json!({"type": "string", "format": "uuid", "description": "Current acceptance-obligation note UUID whose explicit executable ruling to run. Requires commit; cannot combine with test. Runs its versioned command and expected-exit contract in a clean checkout; fleet_report reads this exact receipt."}),
            ),
            (
                "withdraw",
                json!({"type": "integer", "description": "A receipt_id you asked for and no longer need. Cancels it whether it is queued or running; its result is recorded as cancelled. Only the seat that asked can withdraw it. A newer request for the same ruling obligation withdraws the older queued one by itself."}),
            ),
        ],
        &["fleet", "session_id"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_FLEET_CHECK),
        Cow::Borrowed(
            "Run the declared fleet check, a focused `test`, or an explicit project `command`. \
             Shared checkout access is serialized; declared operations acquire their own host resources. \
             By default return immediately with a persisted `receipt_id`, `how` waiting/running, \
             and no passing verdict; mike mails the real result on completion. Carry on with other \
             work; never sleep-poll. Optional wait_seconds is capped at 90. Finished replies include \
             exit_code, passed, output_tail and diagnostics sorted into mine/others/unowned. \
             Command receipts are not acceptance proof. Explicit commands run afresh; status only \
             reads. Reuse needs declared inputs and a cache_key covering external dependencies; working-tree commands run afresh. `test` uses \
             check.test_command with one quoted test name. The default checkout includes seats' \
             uncommitted edits; `commit` selects an isolated reusable historical checkout. `ruling` \
             runs an acceptance contract at explicit commit. Execution is not resumed after a proxy \
             restart. No Bash call is intercepted. Returns {check: {...}}; select roots at /check.",
        ),
        schema,
    )
}

fn where_tool() -> Tool {
    let schema = object_schema(
        vec![
            fleet_property(),
            (
                "symbol",
                json!({"type": "string", "description": "A name to find the definition of: a function, type, class, module, component or constant. Exact, case-sensitive."}),
            ),
            (
                "path",
                json!({"type": "string", "description": "A file or directory, relative to the repository: lists the definitions in it (or, with `symbol`, only that one there)."}),
            ),
        ],
        &["fleet"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_FLEET_WHERE),
        Cow::Borrowed(
            "Find where a name is defined in the fleet's checkout, in any language, in tracked \
             and untracked files. Read-only. Give `symbol`, `path` or both. Returns `{where: \
             {...}}`: each definition with its file, line and the line's text, and `files_named`, \
             the files whose name is the symbol. At most 200 definitions; `definitions_truncated` \
             is true when there were more, so narrow with `path`. Includes textual references, \
             bounded source context with full-file paths/hashes, commit and dirty state. These \
             are live text matches, not a semantic call graph. `select` roots at `/where`.",
        ),
        schema,
    )
}

#[derive(Debug, Deserialize)]
struct CheckArgs {
    fleet: String,
    session_id: String,
    #[serde(default)]
    status: bool,
    #[serde(default)]
    wait_seconds: Option<u64>,
    #[serde(default)]
    test: Option<String>,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    commit: Option<String>,
    #[serde(default)]
    ruling: Option<String>,
    #[serde(default)]
    withdraw: Option<i64>,
}

fn check_body(args: &CheckArgs) -> Value {
    let mut body = json!({"session": args.session_id, "status": args.status});
    if let Some(command) = &args.command {
        body["command"] = json!(command);
    }
    if let Some(test) = &args.test {
        body["test"] = json!(test);
    }
    if let Some(ruling) = &args.ruling {
        body["ruling"] = json!(ruling);
    }
    if let Some(commit) = &args.commit {
        body["commit"] = json!(commit);
    }
    if let Some(seconds) = args.wait_seconds {
        body["wait_seconds"] = json!(seconds.min(MAX_WAIT_SECONDS));
    }
    if let Some(receipt) = args.withdraw {
        body["withdraw"] = json!(receipt);
    }
    body
}

#[derive(Debug, Deserialize)]
struct WhereArgs {
    fleet: String,
    #[serde(default)]
    symbol: Option<String>,
    #[serde(default)]
    path: Option<String>,
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    match name {
        TOOL_FLEET_CHECK => {
            let args = parse_args_required::<CheckArgs>(arguments)?;
            let body = check_body(&args);
            let value = client
                .check_fleet(&args.fleet, &body)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("check", value))
        }
        TOOL_FLEET_WHERE => {
            let args = parse_args_required::<WhereArgs>(arguments)?;
            let q: Vec<(&str, String)> = [("symbol", args.symbol), ("path", args.path)]
                .into_iter()
                .filter_map(|(k, v)| v.map(|v| (k, v)))
                .collect();
            let value = client
                .where_in_fleet(&args.fleet, &q)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("where", value))
        }
        other => Err(RmcpError::invalid_params(
            format!("unknown fleet tool: {other}"),
            None,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_tools_are_named_and_neither_is_mutating() {
        let tools = fleet_check_tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        assert_eq!(names, FLEET_CHECK_TOOL_NAMES);
        for tool in &tools {
            let text = tool.description.as_deref().unwrap_or_default();
            assert!(!text.contains("Mutating."), "{}", tool.name);
        }
        assert!(owns(TOOL_FLEET_WHERE) && !owns("fleet_commit"));
    }

    fn args(wait: Option<u64>) -> CheckArgs {
        CheckArgs {
            fleet: "f".into(),
            session_id: "s".into(),
            status: false,
            wait_seconds: wait,
            test: None,
            command: None,
            commit: None,
            ruling: None,
            withdraw: None,
        }
    }

    #[test]
    fn a_named_test_and_commit_reach_the_check_body_only_when_given() {
        let plain = check_body(&args(None));
        assert!(plain.get("test").is_none() && plain.get("commit").is_none());
        let focused = CheckArgs {
            test: Some("mike::cli x".into()),
            commit: Some("abc1234".into()),
            ..args(None)
        };
        let body = check_body(&focused);
        assert_eq!(body["test"], json!("mike::cli x"));
        assert_eq!(body["commit"], json!("abc1234"));
        let properties = check_tool().input_schema.get("properties").cloned();
        assert!(properties.is_some_and(|p| p.get("test").is_some() && p.get("commit").is_some()));
    }

    #[test]
    fn a_withdrawn_receipt_reaches_the_check_body_only_when_given() {
        assert!(check_body(&args(None)).get("withdraw").is_none());
        let body = check_body(&CheckArgs {
            withdraw: Some(1461),
            ..args(None)
        });
        assert_eq!(body["withdraw"], json!(1461));
        assert!(
            check_tool().input_schema["properties"]
                .get("withdraw")
                .is_some()
        );
    }

    #[test]
    fn an_explicit_command_reaches_the_check_without_waiting() {
        let body = check_body(&CheckArgs {
            command: Some("cargo nextest run -p gg --test mix".into()),
            ..args(Some(0))
        });
        assert_eq!(body["command"], json!("cargo nextest run -p gg --test mix"));
        assert_eq!(body["wait_seconds"], json!(0));
        assert!(
            check_tool().input_schema["properties"]
                .get("command")
                .is_some()
        );
    }

    #[test]
    fn a_wait_is_sent_to_the_check_and_held_to_the_longest_the_tool_allows() {
        assert_eq!(check_body(&args(None)).get("wait_seconds"), None);
        assert_eq!(check_body(&args(Some(120)))["wait_seconds"], json!(90));
        assert_eq!(check_body(&args(Some(5000)))["wait_seconds"], json!(90));
        assert_eq!(check_body(&args(Some(0)))["wait_seconds"], json!(0));
    }

    #[test]
    fn the_check_tool_offers_a_wait_and_stays_under_the_description_cap() {
        let tool = check_tool();
        let properties = tool
            .input_schema
            .get("properties")
            .and_then(Value::as_object);
        assert!(properties.is_some_and(|p| p.contains_key("wait_seconds")));
        let text = tool.description.as_deref().unwrap_or_default();
        assert!(text.len() <= 2048, "{} chars", text.len());
        assert!(text.contains("wait_seconds"));
    }
}
