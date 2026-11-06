use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{object_schema, parse_args_required, wrap_success};
use crate::client::InspectClient;
use crate::error::McpError;

pub const TOOL_WHAT_CHANGED: &str = "what_changed";

pub fn changes_tools() -> Vec<Tool> {
    vec![what_changed_tool()]
}

pub fn owns(name: &str) -> bool {
    name == TOOL_WHAT_CHANGED
}

fn what_changed_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "view",
                json!({"type": "string", "enum": ["changes", "standing", "whose", "mine", "diff"], "description": "changes: what others changed since you last looked. standing: where things stand. whose: who owns and last changed one path. mine: your own uncommitted files. diff: bounded unified diff for commit, path since your place, or your recorded own paths."}),
            ),
            (
                "session_id",
                json!({"type": "string", "description": "Your own session id. Needed for changes and standing; whose can use `checkout` instead."}),
            ),
            (
                "since",
                json!({"type": "string", "description": "changes only: `place` (default, since your last look), an RFC 3339 time, or a commit id of at least four hex digits."}),
            ),
            (
                "scope",
                json!({"type": "string", "enum": ["mine", "all", "paths"], "description": "changes only: `mine` (default) is your own files, your task's files and files you have read; `all` is every file; `paths` is the files listed in `paths`."}),
            ),
            (
                "paths",
                json!({"type": "string", "description": "changes only: comma-separated files or folders, relative to the checkout."}),
            ),
            (
                "path",
                json!({"type": "string", "description": "whose or diff: the file, relative to the checkout or absolute."}),
            ),
            (
                "commit",
                json!({"type": "string", "description": "diff only: a commit id of at least four hex digits; shows its first-parent diff."}),
            ),
            (
                "line_limit",
                json!({"type": "integer", "minimum": 1, "maximum": 2000, "description": "diff only: maximum output lines (default 400); truncated says whether anything was omitted."}),
            ),
            (
                "checkout",
                json!({"type": "string", "description": "The repository root. Default: your fleet's checkout, else the git root of your working folder."}),
            ),
        ],
        &["view"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_WHAT_CHANGED),
        Cow::Borrowed(
            "Ask mike what changed in your checkout and where things stand, in place of running \
             git status, git log or git diff to learn what others changed. Reads mike's record of \
             edits, shell changes, commits, pushes, check verdicts, notes and mail, each with who \
             made it. view=changes returns the lines others produced since you last looked, for \
             your own files, your task's files and files you have read (scope=all or scope=paths \
             widen it), and moves your place so the next call returns only what is newer; \
             `since` takes a time or a commit instead. view=standing returns your uncommitted \
             files and how many changes wait; a fleet seat also gets its task, state, head \
             against the remote and the latest check, and a session outside a fleet gets the \
             other sessions working in the checkout with their uncommitted files and the five \
             newest commits. view=whose with `path` returns the owning task in a running fleet, \
             who last changed the file and whether that change is committed. view=mine returns \
             your own uncommitted paths with the time of each last change, whether the check \
             ran after your last change and its verdict, and the check and verdict lines on \
             those paths. view=diff returns unified_diff, truncated, source and unavailable; commit selects a commit, path selects recorded blobs since your place, otherwise your recorded own paths in the shared tree. Refuses a \
             missing or unknown session, an unknown view or commit, and a session with no git \
             checkout; says so when the record is switched off (`change_record` in /settings). \
             Returns the answer as `{changes: ...}`; `select` roots at `/changes`.",
        ),
        schema,
    )
}

#[derive(Debug, Deserialize)]
struct WhatChangedArgs {
    view: String,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    since: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    paths: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    checkout: Option<String>,
    #[serde(default)]
    commit: Option<String>,
    #[serde(default)]
    line_limit: Option<usize>,
}

fn query_of(args: WhatChangedArgs) -> Vec<(&'static str, String)> {
    [
        ("view", Some(args.view)),
        ("session", args.session_id),
        ("since", args.since),
        ("scope", args.scope),
        ("paths", args.paths),
        ("path", args.path),
        ("checkout", args.checkout),
        ("commit", args.commit),
        ("line_limit", args.line_limit.map(|n| n.to_string())),
    ]
    .into_iter()
    .filter_map(|(key, value)| value.map(|v| (key, v)))
    .collect()
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    match name {
        TOOL_WHAT_CHANGED => {
            let args = parse_args_required::<WhatChangedArgs>(arguments)?;
            let query = query_of(args);
            let value = client
                .what_changed(&query)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("changes", value))
        }
        other => Err(RmcpError::invalid_params(
            format!("unknown changes tool: {other}"),
            None,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tool_is_read_only_and_its_description_stays_under_the_cap() {
        let tool = what_changed_tool();
        let text = tool.description.as_deref().unwrap_or_default();
        assert!(text.len() < 2048, "{} chars", text.len());
        assert!(!text.contains("Mutating."));
        assert!(text.contains("git status"));
        for view in ["changes", "standing", "whose", "mine"] {
            assert!(text.contains(&format!("view={view}")), "{view}");
        }
        assert!(owns(TOOL_WHAT_CHANGED) && !owns("fleet_check"));
    }

    #[test]
    fn only_the_arguments_given_are_sent_to_the_route() {
        let args = WhatChangedArgs {
            view: "changes".into(),
            session_id: Some("abc".into()),
            since: None,
            scope: Some("all".into()),
            paths: None,
            path: None,
            checkout: None,
            commit: None,
            line_limit: None,
        };
        let query = query_of(args);
        assert_eq!(
            query,
            vec![
                ("view", "changes".to_owned()),
                ("session", "abc".to_owned()),
                ("scope", "all".to_owned())
            ]
        );
    }
}
