use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{object_schema, parse_args_required, wrap_success};
use crate::client::InspectClient;
use crate::error::McpError;

pub const TOOL_LIST_FLEET_CHANGES: &str = "list_fleet_changes";
pub const TOOL_LIST_FLEET_RESULTS: &str = "list_fleet_results";
pub const TOOL_FLEET_COMMIT: &str = "fleet_commit";
pub const FLEET_TREE_TOOL_NAMES: &[&str] = &[
    TOOL_LIST_FLEET_CHANGES,
    TOOL_LIST_FLEET_RESULTS,
    TOOL_FLEET_COMMIT,
];

pub fn fleet_tree_tools() -> Vec<Tool> {
    vec![changes_tool(), results_tool(), commit_tool()]
}

pub fn owns(name: &str) -> bool {
    FLEET_TREE_TOOL_NAMES.contains(&name)
}

fn fleet_property() -> (&'static str, Value) {
    (
        "fleet",
        json!({"type": "string", "description": "The fleet id or slug (get_fleet)."}),
    )
}

fn changes_tool() -> Tool {
    let schema = object_schema(
        vec![
            fleet_property(),
            (
                "since",
                json!({"type": "string", "description": "Only changes seen at or after this UTC time (RFC 3339). Default: the last six hours."}),
            ),
            (
                "path",
                json!({"type": "string", "description": "Only paths starting with this text, relative to the repository."}),
            ),
            (
                "limit",
                json!({"type": "integer", "description": "How many changes, newest first. Default 100, at most 1000."}),
            ),
        ],
        &["fleet"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_LIST_FLEET_CHANGES),
        Cow::Borrowed(
            "List the changes in a running fleet's checkout, newest first, whatever made them. \
             Read-only. Each change: the path; whether it was added, modified or deleted; the \
             content hash; when it was seen; the proven author seat, or unknown attribution with \
             inferred candidates; the owning task and owner_seat separately; and an outside-task \
             note only for proven writes. `watching` is true while the checkout is being read. \
             Returns `{changes: {...}}`; `select` roots at `/changes`.",
        ),
        schema,
    )
}

fn results_tool() -> Tool {
    let schema = object_schema(
        vec![
            fleet_property(),
            (
                "command",
                json!({"type": "string", "description": "Only shell commands containing this text, for example `cx check` or `nextest`."}),
            ),
            (
                "paths",
                json!({"type": "string", "description": "Comma-separated path prefixes to display in changed_since. Does not prove dependencies or restrict invalidation: any checkout change since command start invalidates. Default: show all paths."}),
            ),
            (
                "since",
                json!({"type": "string", "description": "Only commands started at or after this UTC time (RFC 3339). Default: the last six hours."}),
            ),
            (
                "limit",
                json!({"type": "integer", "description": "How many commands, newest first. Default 20, at most 200."}),
            ),
            (
                "include_output",
                json!({"type": "boolean", "description": "true adds output tail, use_id, session, times and tool error flag. For full command/output use list_session_tool_uses with that session, include_results true, and find the use_id. Default false keeps rows lean."}),
            ),
        ],
        &["fleet"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_LIST_FLEET_RESULTS),
        Cow::Borrowed(
            "List captured Bash commands for a current or historical fleet, newest first. \
             Read-only. still_valid is null when reuse is unproven, false after any observed \
             checkout change since command start or in update mode. Raw captures have no verified \
             commit binding or dependency closure; no observed change is not proof of validity. \
             Read validity_provenance and inspect recorded output; tool success is not a test pass. \
             paths filters changed_since only. running means no captured completion; waiting and \
             waiting_seconds describe inferred slot contention, not observed process state. \
             completion_unknown marks an unconfirmed background task. include_output adds the \
             output tail, use_id, session, times and tool error flag, with full command/output \
             available through list_session_tool_uses. Returns `{results: {...}}`; select roots at /results.",
        ),
        schema,
    )
}

fn commit_tool() -> Tool {
    let schema = object_schema(
        vec![
            fleet_property(),
            (
                "session_id",
                json!({"type": "string", "description": "Your own session id: the seat whose task's files are committed."}),
            ),
            (
                "message",
                json!({"type": "string", "description": "The commit message. The seat and its session are appended as a trailer."}),
            ),
            (
                "paths",
                json!({"type": "array", "items": {"type": "string"}, "description": "Files or folders to commit, relative to the repository. Default: every changed file the seat's task owns. A shared file must be named here."}),
            ),
            (
                "allow_empty",
                json!({"type": "boolean", "description": "true writes the commit even when no file changed, for a message that must land, such as a closing `Closes #n`. Default false."}),
            ),
        ],
        &["fleet", "session_id", "message"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_FLEET_COMMIT),
        Cow::Borrowed(
            "Commit your task's changed files to the fleet's branch and push. Mutating. Use it in \
             place of `git add`, `git commit` and `git push`. Default: every changed file your \
             task owns; name `paths` to commit a subset, a shared file or any other file. A named \
             folder takes every changed file under it except files another task owns. It never \
             refuses on a judgement: a path outside your task's files, a check that fails in the \
             files committed, or the fleet's commit check (its formatter, for example) failing on \
             them comes back in `warnings`. The repository's own commit hook runs first and can \
             stop the commit. mike writes a `result` note for the commit: what changed, in which \
             files, how it was verified. Returns `{commit: {seat, task, paths, sha, pushed, \
             push_error, verified, warnings, refused}}`: `refused` holds the reason only when \
             there was nothing to commit (pass `allow_empty: true` to land a message with no \
             file change) or the hook stopped it; `push_error` holds git's words when the commit \
             landed but the push failed. Commits pushed to the branch from outside the fleet are \
             merged in before the push is tried again.",
        ),
        schema,
    )
}

#[derive(Debug, Deserialize)]
struct ChangesArgs {
    fleet: String,
    #[serde(default)]
    since: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct ResultsArgs {
    fleet: String,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    paths: Option<String>,
    #[serde(default)]
    since: Option<String>,
    #[serde(default)]
    limit: Option<i64>,
    #[serde(default)]
    include_output: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct CommitArgs {
    fleet: String,
    session_id: String,
    message: String,
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    allow_empty: bool,
}

fn unpushed(report: &Value) -> Option<(String, String)> {
    let text = |key: &str| report.get(key).and_then(Value::as_str).unwrap_or_default();
    let committed = report.get("sha").is_some_and(Value::is_string);
    let pushed = report
        .get("pushed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    (committed && !pushed && !text("repo").is_empty() && !text("branch").is_empty())
        .then(|| (text("repo").to_string(), text("branch").to_string()))
}

const PUSH_ATTEMPTS: usize = 4;
const PUSH_RETRY: std::time::Duration = std::time::Duration::from_millis(800);
const NO_CREDENTIALS: [&str; 3] = [
    "could not read Username",
    "Authentication failed",
    "Permission denied",
];
const BEHIND_ORIGIN: [&str; 3] = ["fetch first", "non-fast-forward", "[rejected]"];

async fn git_origin(repo: &str, verb: &str, branch: &str) -> Result<(), String> {
    let output = tokio::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args([verb, "-q", "origin", branch])
        .stdin(std::process::Stdio::null())
        .output()
        .await;
    match output {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => Err(String::from_utf8_lossy(&out.stderr).trim().to_string()),
        Err(e) => Err(format!("git {verb} could not run: {e}")),
    }
}

fn behind_origin(git_said: &str) -> bool {
    BEHIND_ORIGIN.iter().any(|sign| git_said.contains(sign))
}

async fn bring_in_origin(
    client: &InspectClient,
    fleet: &str,
    repo: &str,
    branch: &str,
) -> Result<(), String> {
    git_origin(repo, "fetch", branch).await?;
    let merged = client
        .integrate_fleet(fleet)
        .await
        .map_err(|e| e.to_string())?;
    match merged.get("problem").and_then(Value::as_str) {
        Some(why) => Err(why.to_string()),
        None => Ok(()),
    }
}

async fn push_with_retries(
    client: &InspectClient,
    fleet: &str,
    repo: &str,
    branch: &str,
) -> Result<(), String> {
    let mut last = String::new();
    for attempt in 0..PUSH_ATTEMPTS {
        if attempt > 0 {
            tokio::time::sleep(PUSH_RETRY).await;
        }
        match git_origin(repo, "push", branch).await {
            Ok(()) => return Ok(()),
            Err(why) if NO_CREDENTIALS.iter().any(|sign| why.contains(sign)) => return Err(why),
            Err(why) => {
                if behind_origin(&why) {
                    bring_in_origin(client, fleet, repo, branch).await?;
                }
                last = why;
            }
        }
    }
    Err(last)
}

async fn push_from_this_session(client: &InspectClient, fleet: &str, report: &mut Value) {
    let Some((repo, branch)) = unpushed(report) else {
        return;
    };
    let outcome = push_with_retries(client, fleet, &repo, &branch).await;
    let Some(fields) = report.as_object_mut() else {
        return;
    };
    match outcome {
        Ok(()) => {
            fields.insert("pushed".to_string(), Value::Bool(true));
            fields.insert("push_error".to_string(), Value::Null);
        }
        Err(why) => {
            fields.insert("push_error".to_string(), Value::String(why));
        }
    }
}

fn query(items: Vec<(&'static str, Option<String>)>) -> Vec<(&'static str, String)> {
    items
        .into_iter()
        .filter_map(|(k, v)| v.map(|v| (k, v)))
        .collect()
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    match name {
        TOOL_LIST_FLEET_CHANGES => {
            let args = parse_args_required::<ChangesArgs>(arguments)?;
            let q = query(vec![
                ("since", args.since),
                ("path", args.path),
                ("limit", args.limit.map(|l| l.to_string())),
            ]);
            let value = client
                .get_fleet_changes(&args.fleet, &q)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("changes", value))
        }
        TOOL_LIST_FLEET_RESULTS => {
            let args = parse_args_required::<ResultsArgs>(arguments)?;
            let q = query(vec![
                ("command", args.command),
                ("paths", args.paths),
                ("since", args.since),
                ("limit", args.limit.map(|l| l.to_string())),
                (
                    "include_output",
                    args.include_output
                        .filter(|wanted| *wanted)
                        .map(|_| "true".to_string()),
                ),
            ]);
            let value = client
                .get_fleet_results(&args.fleet, &q)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("results", value))
        }
        TOOL_FLEET_COMMIT => {
            let args = parse_args_required::<CommitArgs>(arguments)?;
            let body = json!({
                "session": args.session_id,
                "message": args.message,
                "paths": args.paths,
                "allow_empty": args.allow_empty
            });
            let mut value = client
                .commit_fleet(&args.fleet, &body)
                .await
                .map_err(McpError::into_rmcp)?;
            push_from_this_session(client, &args.fleet, &mut value).await;
            Ok(wrap_success("commit", value))
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
    fn only_a_commit_the_server_could_not_push_is_pushed_from_the_session() {
        let landed = json!({"sha": "abc", "pushed": false, "repo": "/srv/p", "branch": "main"});
        assert_eq!(
            unpushed(&landed),
            Some(("/srv/p".to_string(), "main".to_string()))
        );
        assert_eq!(
            unpushed(&json!({"sha": "abc", "pushed": true, "repo": "/srv/p", "branch": "main"})),
            None
        );
        assert_eq!(
            unpushed(&json!({"sha": null, "pushed": false, "repo": "/srv/p", "branch": "main"})),
            None
        );
        assert_eq!(unpushed(&json!({"sha": "abc", "pushed": false})), None);
    }

    #[test]
    fn a_push_refused_for_being_behind_is_told_from_any_other_refusal() {
        assert!(behind_origin(
            " ! [rejected]        main -> main (fetch first)\nerror: failed to push some refs"
        ));
        assert!(!behind_origin(
            "fatal: could not read Username for 'https://github.com'"
        ));
    }

    #[test]
    fn the_three_tools_are_named_and_only_the_commit_is_mutating() {
        let tools = fleet_tree_tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        assert_eq!(names, FLEET_TREE_TOOL_NAMES);
        for tool in &tools {
            let text = tool.description.as_deref().unwrap_or_default();
            assert_eq!(
                text.contains("Mutating."),
                tool.name == TOOL_FLEET_COMMIT,
                "{}",
                tool.name
            );
        }
        assert!(owns(TOOL_LIST_FLEET_RESULTS) && !owns("get_fleet"));
    }

    #[test]
    fn a_query_keeps_only_the_given_parts() {
        let q = query(vec![("since", None), ("path", Some("crates/".into()))]);
        assert_eq!(q, vec![("path", "crates/".to_string())]);
    }
    #[test]
    fn raw_result_help_describes_unknown_validity_and_precise_full_result_retrieval() {
        let tool = results_tool();
        let text = tool.description.as_deref().unwrap_or_default();
        assert!(text.contains("still_valid is null"));
        assert!(text.contains("since command start"));
        assert!(text.contains("tool success is not a test pass"));
        assert!(!text.contains("can be used as it is"));
        let schema = tool.input_schema.as_ref();
        let paths = schema["properties"]["paths"]["description"]
            .as_str()
            .unwrap_or_default();
        assert!(paths.contains("to display in changed_since"));
        let output = schema["properties"]["include_output"]["description"]
            .as_str()
            .unwrap_or_default();
        assert!(output.contains("list_session_tool_uses"));
        assert!(output.contains("use_id"));
    }
}
