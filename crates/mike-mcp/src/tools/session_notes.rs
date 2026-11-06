use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{object_schema, order_property, parse_args_default, parse_args_required, wrap_success};
use crate::client::{InspectClient, ProjectNotesQuery, SessionNotesQuery, WriteSessionNote};
use crate::error::McpError;

pub const SESSION_NOTES_TOOL_NAMES: &[&str] = &[
    TOOL_WRITE_SESSION_NOTE,
    TOOL_LIST_SESSION_NOTES,
    TOOL_DELETE_SESSION_NOTE,
];

pub const TOOL_WRITE_SESSION_NOTE: &str = "write_session_note";
pub const TOOL_LIST_SESSION_NOTES: &str = "list_session_notes";
pub const TOOL_DELETE_SESSION_NOTE: &str = "delete_session_note";

pub fn session_notes_tools() -> Vec<Tool> {
    vec![
        write_session_note_tool(),
        list_session_notes_tool(),
        delete_session_note_tool(),
    ]
}

pub fn owns(name: &str) -> bool {
    SESSION_NOTES_TOOL_NAMES.contains(&name)
}

fn kind_property() -> Value {
    json!({
        "type": "string",
        "description": "Free text, open set. Conventional kinds: \"note\", \"finding\", \"task\", \"decision\", \"question\"."
    })
}

fn status_property() -> Value {
    json!({
        "type": "string",
        "description": "Free text, open set. Conventional task statuses: \"open\", \"in_progress\", \"blocked\", \"done\"."
    })
}

fn write_session_note_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "session_id",
                json!({
                    "type": "string",
                    "description": "Author session id (a canonical UUID). Omit to use the trusted MCP caller session; required when caller context is unavailable."
                }),
            ),
            (
                "note_uuid",
                json!({
                    "type": "string",
                    "description": "Omit to CREATE a new note. Pass the note_uuid of an existing note (from a prior write_session_note or list_session_notes) to UPDATE it in place instead."
                }),
            ),
            ("kind", kind_property()),
            (
                "title",
                json!({
                    "type": "string",
                    "description": "Required when creating (note_uuid absent). Optional on update — omit to leave the existing title unchanged."
                }),
            ),
            (
                "body",
                json!({
                    "type": "string",
                    "description": "The note's free-text content. Omit on update to leave unchanged; omit on create for an empty body."
                }),
            ),
            ("status", status_property()),
            (
                "tags",
                json!({
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Plain tag strings for filtering (list_session_notes `tag`). Replaces the note's tag list wholesale when sent; omit to leave the existing tags unchanged."
                }),
            ),
            (
                "data",
                json!({
                    "type": "object",
                    "description": "Structured JSON payload. Fleet acceptance obligations may carry ruling: {version: positive integer, command: string, expected_exit_code: 0..125} plus explicit tasks. Only acceptance registers these; the executable contract is immutable, revised through a higher-version successor note. Other notes keep their own schema. REPLACES the note's `data` wholesale when sent (not merged); omit to leave it unchanged."
                }),
            ),
        ],
        &[],
    );
    Tool::new(
        Cow::Borrowed(TOOL_WRITE_SESSION_NOTE),
        Cow::Borrowed(
            "Write one entry to this session's notebook: a finding, task, decision or free-form \
             note. Mutating. Without `note_uuid` it creates a note (`title` required); with \
             `note_uuid` it updates that note in place. On update an omitted field is left \
             unchanged, except `data` and `tags`, which are replaced whole when sent, never \
             merged. Returns the note row: {note_uuid, session_id, kind, title, body, status, \
             tags, data, created_at, updated_at}; when the change record's tail is on, the answer \
             may also carry `since_you_last_looked`, the lines of what others changed since you \
             last looked. Any session in the project can read it with \
             list_session_notes. To close a task: `{session_id, note_uuid, status: \"done\"}`.",
        ),
        schema,
    )
}

fn list_session_notes_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "session_id",
                json!({
                    "type": "string",
                    "description": "Notes of this one session. Required unless `project_path` is given; with both, narrows the project search to this session's notes."
                }),
            ),
            (
                "project_path",
                json!({
                    "type": "string",
                    "description": "Search every session's notes in this project, by the path as recorded by the proxy (see list_projects, get_workspace_roster). Combine with `role`, `kind`, `status`, `tag` or `q`, e.g. `{project_path, role: \"observer\", kind: \"finding\"}`."
                }),
            ),
            (
                "role",
                json!({
                    "type": "string",
                    "description": "Project search only: keep only notes by a session whose declared `role` (set via set_session_goal or register_workspace_session) matches, e.g. \"observer\"."
                }),
            ),
            (
                "name",
                json!({
                    "type": "string",
                    "description": "Project-wide search only: keep only notes authored by a session with this declared display name."
                }),
            ),
            ("kind", kind_property()),
            ("status", status_property()),
            (
                "tag",
                json!({
                    "type": "string",
                    "description": "Keep only notes carrying this tag."
                }),
            ),
            (
                "q",
                json!({
                    "type": "string",
                    "description": "Free-text search over title and body."
                }),
            ),
            (
                "limit",
                json!({
                    "type": "integer",
                    "minimum": 1,
                    "description": "Maximum number of notes to return. Omit for the server default page size."
                }),
            ),
            (
                "offset",
                json!({
                    "type": "integer",
                    "minimum": 0,
                    "description": "Notes to skip, for paging past `limit`. Default 0."
                }),
            ),
            ("order", order_property()),
        ],
        &[],
    );
    Tool::new(
        Cow::Borrowed(TOOL_LIST_SESSION_NOTES),
        Cow::Borrowed(
            "List notes from one session's notebook or from every session in a project. \
             Read-only. Pass `session_id`, `project_path` or both; with neither the call is \
             refused. `session_id` alone reads that session's notes; `project_path` searches all \
             of the project's sessions, and with both the search is narrowed to that session. \
             `kind`, `status`, `tag` and `q` (free text) narrow either search; `role` and `name` \
             apply to the project search only. Paginated with `limit` and `offset`, newest first \
             by default (`order`). Full rows.",
        ),
        schema,
    )
}

fn delete_session_note_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "session_id",
                json!({
                    "type": "string",
                    "description": "Session id that owns the note."
                }),
            ),
            (
                "note_uuid",
                json!({
                    "type": "string",
                    "description": "The note to permanently delete."
                }),
            ),
        ],
        &["session_id", "note_uuid"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_DELETE_SESSION_NOTE),
        Cow::Borrowed(
            "Permanently delete one of this session's notes by `note_uuid`. Mutating. Hard \
             delete: the row is gone. 404 when the note_uuid is unknown. Returns {deleted: true, \
             warnings: []}. To mark a task done rather than erase it, update `status` with \
             write_session_note.",
        ),
        schema,
    )
}

#[derive(Debug, Deserialize)]
pub(crate) struct WriteSessionNoteArgs {
    pub(crate) session_id: String,
    #[serde(default)]
    pub(crate) note_uuid: Option<String>,
    #[serde(default)]
    pub(crate) kind: Option<String>,
    #[serde(default)]
    pub(crate) title: Option<String>,
    #[serde(default)]
    pub(crate) body: Option<String>,
    #[serde(default)]
    pub(crate) status: Option<String>,
    #[serde(default)]
    pub(crate) tags: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) data: Option<Value>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct ListSessionNotesArgs {
    #[serde(default)]
    pub(crate) session_id: Option<String>,
    #[serde(default)]
    pub(crate) project_path: Option<String>,
    #[serde(default)]
    pub(crate) role: Option<String>,
    #[serde(default)]
    pub(crate) name: Option<String>,
    #[serde(default)]
    pub(crate) kind: Option<String>,
    #[serde(default)]
    pub(crate) status: Option<String>,
    #[serde(default)]
    pub(crate) tag: Option<String>,
    #[serde(default)]
    pub(crate) q: Option<String>,
    #[serde(default)]
    pub(crate) limit: Option<u64>,
    #[serde(default)]
    pub(crate) offset: Option<u64>,
    #[serde(default)]
    pub(crate) order: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DeleteSessionNoteArgs {
    pub(crate) session_id: String,
    pub(crate) note_uuid: String,
}

async fn list_session_notes_call(
    client: &InspectClient,
    args: &ListSessionNotesArgs,
) -> Result<Value, McpError> {
    match (&args.project_path, &args.session_id) {
        (Some(project_path), _) => {
            client
                .list_project_notes(
                    project_path,
                    ProjectNotesQuery {
                        role: args.role.as_deref(),
                        name: args.name.as_deref(),
                        kind: args.kind.as_deref(),
                        status: args.status.as_deref(),
                        tag: args.tag.as_deref(),
                        q: args.q.as_deref(),
                        session_uuid: args.session_id.as_deref(),
                        limit: args.limit,
                        offset: args.offset,
                        order: args.order.as_deref(),
                    },
                )
                .await
        }
        (None, Some(session_id)) => {
            client
                .list_session_notes(
                    session_id,
                    SessionNotesQuery {
                        kind: args.kind.as_deref(),
                        status: args.status.as_deref(),
                        tag: args.tag.as_deref(),
                        q: args.q.as_deref(),
                        limit: args.limit,
                        offset: args.offset,
                        order: args.order.as_deref(),
                    },
                )
                .await
        }
        (None, None) => unreachable!("call_tool validates session_id or project_path first"),
    }
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    match name {
        TOOL_WRITE_SESSION_NOTE => {
            let args = parse_args_required::<WriteSessionNoteArgs>(arguments)?;
            let value = client
                .write_session_note(WriteSessionNote {
                    session_id: &args.session_id,
                    note_uuid: args.note_uuid.as_deref(),
                    kind: args.kind.as_deref(),
                    title: args.title.as_deref(),
                    body: args.body.as_deref(),
                    status: args.status.as_deref(),
                    tags: args.tags.as_deref(),
                    data: args.data.as_ref(),
                })
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("note", value))
        }
        TOOL_LIST_SESSION_NOTES => {
            let args = parse_args_default::<ListSessionNotesArgs>(arguments)?;
            if args.session_id.is_none() && args.project_path.is_none() {
                return Err(RmcpError::invalid_params(
                    "list_session_notes requires session_id, project_path, or both",
                    None,
                ));
            }
            let value = list_session_notes_call(client, &args)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("notes", value))
        }
        TOOL_DELETE_SESSION_NOTE => {
            let args = parse_args_required::<DeleteSessionNoteArgs>(arguments)?;
            let value = client
                .delete_session_note(&args.session_id, &args.note_uuid)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("result", value))
        }
        other => Err(RmcpError::invalid_params(
            format!("unknown session notes tool: {other}"),
            None,
        )),
    }
}
