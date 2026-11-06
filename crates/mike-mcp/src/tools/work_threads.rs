use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use super::{object_schema, parse_args_default, parse_args_required, wrap_success};
use crate::client::{InspectClient, ThreadGetQuery, ThreadListQuery};
use crate::error::McpError;

pub const WORK_THREADS_TOOL_NAMES: &[&str] = &[
    TOOL_LIST_THREADS,
    TOOL_GET_THREAD,
    TOOL_CHECKPOINT_THREAD,
    TOOL_ADD_THREAD_ENTRY,
];

pub const TOOL_LIST_THREADS: &str = "work_thread_list";
pub const TOOL_GET_THREAD: &str = "work_thread_get";
pub const TOOL_CHECKPOINT_THREAD: &str = "work_thread_checkpoint";
pub const TOOL_ADD_THREAD_ENTRY: &str = "work_thread_add_entry";

pub fn work_threads_tools() -> Vec<Tool> {
    vec![
        list_threads_tool(),
        get_thread_tool(),
        checkpoint_thread_tool(),
        add_thread_entry_tool(),
    ]
}

pub fn owns(name: &str) -> bool {
    WORK_THREADS_TOOL_NAMES.contains(&name)
}

fn text(description: &str) -> Value {
    json!({"type": "string", "description": description})
}

fn project_property() -> Value {
    text(
        "The project, as its UUID or a registered path (list_project_locations shows both). Omit to use the project of the calling session.",
    )
}

fn slug_property() -> Value {
    text("The thread's slug: letters, digits, '.', '_' and '-', for example `frontend-motion`.")
}

fn session_property() -> Value {
    text(
        "The session writing this. Omit to use the calling session. A thread keeps a link to every session that wrote to it.",
    )
}

fn list_threads_tool() -> Tool {
    let schema = object_schema(
        vec![
            ("project", project_property()),
            (
                "status",
                text(
                    "Only threads in this status: active, implementing, review, parked or done. Several may be joined with commas, for example `active,implementing`.",
                ),
            ),
            (
                "limit",
                json!({"type": "integer", "minimum": 1, "maximum": 500, "description": "Threads per page. Default 200."}),
            ),
            (
                "offset",
                json!({"type": "integer", "minimum": 0, "description": "Threads to skip, for paging past `limit`. Default 0."}),
            ),
        ],
        &[],
    );
    Tool::new(
        Cow::Borrowed(TOOL_LIST_THREADS),
        Cow::Borrowed(
            "List a project's work threads, most recently updated first. Read-only. A work \
             thread is a unit of multi-session work held in mike; it replaces worklog.md. It is \
             not a comms thread between sessions (read_thread). Each row has the slug, title, \
             status, tldr, next_up and updated_at. Returns {threads, total}. Use \
             work_thread_get for one work thread in full.",
        ),
        schema,
    )
}

fn get_thread_tool() -> Tool {
    let schema = object_schema(
        vec![
            ("project", project_property()),
            ("slug", slug_property()),
            (
                "history_limit",
                json!({"type": "integer", "minimum": 1, "maximum": 500, "description": "History entries to return, newest first. Default 20."}),
            ),
            (
                "history_before",
                json!({"type": "integer", "description": "Return only entries older than this entry number; pass the `history_next_before_ord` of the previous answer to page back."}),
            ),
        ],
        &["slug"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_GET_THREAD),
        Cow::Borrowed(
            "Read one work thread (the record that replaces worklog.md, not a comms thread): \
             identity, current state (tldr, where we are, next, blockers, open \
             questions, decisions, files), acceptance criteria, dependencies, the newest history \
             entries (checkpoints, briefs, handoffs) and the sessions that worked on it. Read-only. \
             Page the history with `history_before`.",
        ),
        schema,
    )
}

fn checkpoint_properties() -> Vec<(&'static str, Value)> {
    vec![
        ("project", project_property()),
        ("slug", slug_property()),
        (
            "tldr",
            text("What a session picking this up must know, in at most 600 characters. Required."),
        ),
        (
            "title",
            text("Required the first time, to create the thread; afterwards it renames it."),
        ),
        (
            "goal",
            text("What the thread is for. Send an empty string to clear it."),
        ),
        (
            "status",
            text(
                "active, implementing, review, parked or done. `review` is refused unless the thread has acceptance criteria.",
            ),
        ),
        (
            "branch",
            text("The git branch. Send an empty string to clear it."),
        ),
        (
            "worktree",
            text(
                "The git worktree, as a path relative to the project root. Send an empty string to clear it.",
            ),
        ),
        ("where_we_are", text("Where the work stands.")),
        ("next_up", text("What happens next.")),
        ("blockers", text("What blocks the thread.")),
        ("open_questions", text("What is undecided.")),
        ("decisions", text("Decisions in force.")),
        (
            "files",
            json!({"type": "array", "items": {"type": "string"}, "description": "Files the next session must read, relative to the project root."}),
        ),
        (
            "depends_on",
            json!({"type": "array", "items": {"type": "string"}, "description": "Slugs of the threads this one depends on. Replaces the whole list when sent."}),
        ),
        (
            "acceptance_criteria",
            text(
                "The checks that must hold before the thread is done. Replaces the criteria when sent.",
            ),
        ),
        ("session_id", session_property()),
        (
            "commit_sha",
            text("The commit the checkpoint was written at."),
        ),
        (
            "contribution",
            text("One line, at most 300 characters, saying what this session did for the thread."),
        ),
    ]
}

fn checkpoint_thread_tool() -> Tool {
    Tool::new(
        Cow::Borrowed(TOOL_CHECKPOINT_THREAD),
        Cow::Borrowed(
            "Checkpoint a work thread, the record that replaces writing worklog.md, creating it \
             when `title` is sent for a new slug. Mutating. \
             The state fields (tldr, where_we_are, next_up, blockers, open_questions, decisions, \
             files) replace the current state whole, so send every one you want to keep; the \
             state replaced is kept in the history. Also sets status, branch, worktree, goal, \
             dependencies and acceptance criteria when sent, and links this session. Returns the \
             thread plus created, state_replaced and entry_ord.",
        ),
        object_schema(checkpoint_properties(), &["slug", "tldr"]),
    )
}

fn add_thread_entry_tool() -> Tool {
    let schema = object_schema(
        vec![
            ("project", project_property()),
            ("slug", slug_property()),
            (
                "kind",
                json!({"type": "string", "enum": ["checkpoint", "decision", "finding", "brief", "handoff", "note"], "description": "What the entry is. A brief or handoff is what a session writes for the next one."}),
            ),
            ("body", text("The entry in full.")),
            (
                "summary",
                text("One line for lists. Defaults to the first line of `body`."),
            ),
            ("session_id", session_property()),
            ("commit_sha", text("The commit the entry was written at.")),
            (
                "data",
                json!({"type": "object", "description": "Structured payload to keep with the entry."}),
            ),
        ],
        &["slug", "kind", "body"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_ADD_THREAD_ENTRY),
        Cow::Borrowed(
            "Append an entry to a work thread's history without changing its state: a decision, \
             finding, brief, handoff or note. Mutating. Entries are never edited. Returns the \
             entry with its number.",
        ),
        schema,
    )
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct ListThreadsArgs {
    #[serde(default)]
    project: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    limit: Option<u64>,
    #[serde(default)]
    offset: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GetThreadArgs {
    #[serde(default)]
    project: Option<String>,
    slug: String,
    #[serde(default)]
    history_limit: Option<u64>,
    #[serde(default)]
    history_before: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CheckpointArgs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    project: Option<String>,
    slug: String,
    tldr: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    goal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    worktree: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    where_we_are: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    next_up: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    blockers: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    open_questions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    decisions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    files: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    depends_on: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    acceptance_criteria: Option<String>,
    #[serde(default, alias = "session_id", skip_serializing_if = "Option::is_none")]
    session_uuid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    commit_sha: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    contribution: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EntryArgs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    project: Option<String>,
    #[serde(skip_serializing)]
    slug: String,
    kind: String,
    body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    #[serde(default, alias = "session_id", skip_serializing_if = "Option::is_none")]
    session_uuid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    commit_sha: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

fn project_of(project: &Option<String>, tool: &str) -> Result<String, RmcpError> {
    project
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            RmcpError::invalid_params(
                format!("{tool} needs `project`: pass the project UUID or a registered path (list_project_locations shows both)"),
                None,
            )
        })
}

fn body_of<T: Serialize>(args: &T) -> Result<Value, RmcpError> {
    serde_json::to_value(args)
        .map_err(|error| RmcpError::invalid_params(format!("invalid arguments: {error}"), None))
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    match name {
        TOOL_LIST_THREADS => {
            let args = parse_args_default::<ListThreadsArgs>(arguments)?;
            let project = project_of(&args.project, name)?;
            let value = client
                .list_threads(ThreadListQuery {
                    project: &project,
                    status: args.status.as_deref(),
                    limit: args.limit,
                    offset: args.offset,
                })
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("threads", value))
        }
        TOOL_GET_THREAD => {
            let args = parse_args_required::<GetThreadArgs>(arguments)?;
            let project = project_of(&args.project, name)?;
            let value = client
                .get_thread(ThreadGetQuery {
                    project: &project,
                    slug: &args.slug,
                    history_limit: args.history_limit,
                    history_before: args.history_before,
                })
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("thread", value))
        }
        TOOL_CHECKPOINT_THREAD => {
            let args = parse_args_required::<CheckpointArgs>(arguments)?;
            project_of(&args.project, name)?;
            let value = client
                .checkpoint_thread(&body_of(&args)?)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("checkpoint", value))
        }
        TOOL_ADD_THREAD_ENTRY => {
            let args = parse_args_required::<EntryArgs>(arguments)?;
            project_of(&args.project, name)?;
            let value = client
                .add_thread_entry(&args.slug, &body_of(&args)?)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("entry", value))
        }
        other => Err(RmcpError::invalid_params(
            format!("unknown work-thread tool: {other}"),
            None,
        )),
    }
}

#[cfg(test)]
#[path = "work_threads_tests.rs"]
mod work_threads_tests;
