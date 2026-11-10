use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{
    IdStringArgs, object_schema, order_property, parse_args_default, parse_args_required,
    wrap_success, wrap_success_with_note,
};
use crate::client::InspectClient;
use crate::error::McpError;

pub const MEMORY_TOOL_NAMES: &[&str] = &[
    TOOL_LIST_AGENT_DISPATCHES,
    TOOL_RECALL_PROJECT_CONTEXT,
    TOOL_RECALL_SIMILAR_TURNS,
    TOOL_RECORD_LESSON,
    TOOL_LIST_LESSONS,
    TOOL_DELETE_LESSON,
];

pub const TOOL_LIST_AGENT_DISPATCHES: &str = "list_agent_dispatches";
pub const TOOL_RECALL_PROJECT_CONTEXT: &str = "recall_project_context";
pub const TOOL_RECALL_SIMILAR_TURNS: &str = "recall_similar_turns";
pub const TOOL_RECORD_LESSON: &str = "record_lesson";
pub const TOOL_LIST_LESSONS: &str = "list_lessons";
pub const TOOL_DELETE_LESSON: &str = "delete_lesson";

pub fn memory_tools() -> Vec<Tool> {
    vec![
        list_agent_dispatches_tool(),
        recall_project_context_tool(),
        recall_similar_turns_tool(),
        record_lesson_tool(),
        list_lessons_tool(),
        delete_lesson_tool(),
    ]
}

pub fn owns(name: &str) -> bool {
    MEMORY_TOOL_NAMES.contains(&name)
}

fn list_agent_dispatches_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "project",
                json!({
                    "type": "string",
                    "description": "Optional exact-match filter on `projects.projects.project_path`."
                }),
            ),
            (
                "since",
                json!({
                    "type": "string",
                    "description": "RFC 3339 lower bound. Alias for `date_from`; `date_from` wins if both given."
                }),
            ),
            (
                "date_from",
                json!({
                    "type": "string",
                    "description": "RFC 3339 lower bound on the parent request's created_at (inclusive). Absent → 7-day default window."
                }),
            ),
            (
                "date_to",
                json!({
                    "type": "string",
                    "description": "RFC 3339 upper bound on the parent request's created_at (exclusive). Absent → now (paired with 7-day default)."
                }),
            ),
            (
                "limit",
                json!({
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 500,
                    "description": "Maximum rows per page (default 50, max 500)."
                }),
            ),
            (
                "offset",
                json!({
                    "type": "integer",
                    "minimum": 0,
                    "description": "Skip this many rows. Default 0."
                }),
            ),
            ("order", order_property()),
        ],
        &[],
    );
    Tool::new(
        Cow::Borrowed(TOOL_LIST_AGENT_DISPATCHES),
        Cow::Borrowed(
            "List Agent tool dispatches (sub-agent spawns), paginated. Each row: `description`, \
             `brief_char_len`, `brief_sha256`. For the full prompt use \
             `get_request_body(parent_request_id)`.",
        ),
        schema,
    )
}

fn recall_project_context_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "query",
                json!({"type": "string", "description": "Optional task query; includes relevant artifact, notebook, lesson and turn references with full-content read tool."}),
            ),
            (
                "include_own_turn",
                json!({"type": "boolean", "description": "Also return the turn that is making this call (and its corrections); omitted, the newest turn of the calling session is left out."}),
            ),
            (
                "project_path",
                json!({
                    "type": "string",
                    "description": "Stable project UUID or unambiguous registered project path."
                }),
            ),
        ],
        &["project_path"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_RECALL_PROJECT_CONTEXT),
        Cow::Borrowed(
            "Return a project's memory in one call: the 5 most recent corrected turns, the 3 most \
             recent shipped turns and the active lessons, all in full. Use it at session start.",
        ),
        schema,
    )
}

fn recall_similar_turns_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "project_path",
                json!({
                    "type": "string",
                    "description": "Stable project UUID or unambiguous registered project path."
                }),
            ),
            (
                "query",
                json!({
                    "type": "string",
                    "description": "Free-text query for semantic/FTS recall."
                }),
            ),
            (
                "limit",
                json!({
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 50,
                    "description": "Maximum rows (default 5, max 50)."
                }),
            ),
            (
                "include_own_turn",
                json!({"type": "boolean", "description": "Also return the turn that is making this call; omitted, the newest turn of the calling session is left out."}),
            ),
            (
                "order",
                json!({"type": "string", "enum": ["relevance", "recent"], "description": "relevance (default) ranks by similarity; recent takes the strongest matches and lists them newest first, without the reranker."}),
            ),
        ],
        &["project_path", "query"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_RECALL_SIMILAR_TURNS),
        Cow::Borrowed(
            "Find the operator turns of a project that resemble a query: by embedding similarity \
             when embeddings are available, otherwise by hybrid text and semantic search. Returns \
             `{turns, retrieval_path, degraded, pending_turns?, rerank}`; `turns` are full \
             operator-turn rows. `rerank` is `{state: reranked|loading|off|unavailable, \
             ready_in_s, detail?}`: when state is reranked, a reranker model has reordered the \
             turns (each then has `rerank_score`); otherwise `detail` says why they are in plain \
             order. `degraded: true` means embeddings were unavailable and full-text search served \
             the result, so ranking is weaker. `pending_turns`, when present, counts turns without \
             embeddings; a non-zero value means ranking improves once the back-fill completes. \
             For questions about the latest or current work, pass `order=recent`.",
        ),
        schema,
    )
}

fn record_lesson_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "project_path",
                json!({
                    "type": "string",
                    "description": "Project this lesson is scoped to. Auto-creates the projects row if missing."
                }),
            ),
            (
                "kind",
                json!({
                    "type": "string",
                    "enum": ["correction", "convention", "warning", "preference"],
                    "description": "Lesson category. CHECK constraint enforces this set in DB."
                }),
            ),
            (
                "text",
                json!({
                    "type": "string",
                    "description": "One durable rule a future session can act on: the rule in the \
                                    imperative, then the trigger that makes it apply, then what to do. \
                                    Keep the evidence that makes it checkable (a command, a path, a \
                                    measured value) and drop the rest. No run narration, no dated recap \
                                    of a session, and no \"this supersedes the earlier version\" prose \
                                    (pass `supersedes` instead). Stored verbatim. A text over 500 \
                                    characters (the `lessons.shape.max_chars` setting), opening with a \
                                    status word such as \"Satisfied\" or \"Left unmet\", or naming a \
                                    session uuid or commit sha as its subject is accepted and reported \
                                    in the result's `shape` field. While the lesson proposer is on it \
                                    is also held at the raw stage and not injected until it is \
                                    rewritten as a rule."
                }),
            ),
            (
                "topic",
                json!({"type":"string", "description":"Optional stable retrieval topic. Prefix shared:<topic> to explicitly share an accepted rule across projects. Omit agent_name for project-wide or shared rules; ordinary topics remain project-scoped."}),
            ),
            (
                "source_turn_id",
                json!({
                    "type": "integer",
                    "minimum": 1,
                    "description": "Optional pointer to the `operator_turns.turn_id` that prompted this lesson."
                }),
            ),
            (
                "agent_name",
                json!({
                    "type": "string",
                    "description": "Optional agent/persona scope (e.g. \"rust-developer\"). These lessons are automatically delivered only to that persona, not primary sessions. Omit for project-wide or shared rules."
                }),
            ),
            (
                "session_id",
                json!({
                    "type": "string",
                    "description": "Session id (a canonical UUID); an agent passes its own. Links the \
                                    lesson to that session and, when the session has an operator turn, \
                                    to that turn, so `source_turn_id` is not needed. Ignored when \
                                    `source_turn_id` is also given (the turn's own session wins)."
                }),
            ),
            (
                "supersedes",
                json!({
                    "type": "integer",
                    "minimum": 1,
                    "description": "Id of an existing lesson (`memory.lessons.id`) that this one supersedes. \
                                    Recorded as a confirmed relation. Rejected with an error if the id does not \
                                    name an existing lesson."
                }),
            ),
        ],
        &["project_path", "kind", "text"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_RECORD_LESSON),
        Cow::Borrowed(
            "Record a lesson (correction, convention, warning or preference) for a project: a \
             standing rule for the next session, not a write-up of this one. Future sessions read \
             lessons through `recall_project_context`, and a few are injected into every session's \
             context, so shape matters more than length. `text` is stored verbatim with no length \
             cap.",
        ),
        schema,
    )
}

fn list_lessons_tool() -> Tool {
    let schema = object_schema(
        vec![
            (
                "project_path",
                json!({
                    "type": "string",
                    "description": "Project path to list lessons for."
                }),
            ),
            ("order", order_property()),
        ],
        &["project_path"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_LIST_LESSONS),
        Cow::Borrowed(
            "List a project's active (non-expired) lessons, newest first by default \
             (`order=desc`); pass `order=asc` for oldest first. Each row carries the full lesson \
             text and metadata.",
        ),
        schema,
    )
}

fn delete_lesson_tool() -> Tool {
    let schema = object_schema(
        vec![(
            "id",
            json!({
                "type": "string",
                "description": "Lesson integer id (the `id` column of `memory.lessons`)."
            }),
        )],
        &["id"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_DELETE_LESSON),
        Cow::Borrowed(
            "Delete a lesson by integer id. Returns `{deleted: true}` on success, \
             404 when the row does not exist.",
        ),
        schema,
    )
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct ListAgentDispatchesArgs {
    #[serde(default)]
    pub(crate) project: Option<String>,
    #[serde(default)]
    pub(crate) since: Option<String>,
    #[serde(default)]
    pub(crate) date_from: Option<String>,
    #[serde(default)]
    pub(crate) date_to: Option<String>,
    #[serde(default)]
    pub(crate) limit: Option<u64>,
    #[serde(default)]
    pub(crate) offset: Option<u64>,
    #[serde(default)]
    pub(crate) order: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RecallProjectContextArgs {
    pub(crate) project_path: String,
    pub(crate) query: Option<String>,
    #[serde(flatten)]
    pub(crate) caller: crate::client::CallerArgs,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RecallSimilarTurnsArgs {
    pub(crate) project_path: String,
    pub(crate) query: String,
    #[serde(default)]
    pub(crate) limit: Option<u64>,
    #[serde(default)]
    pub(crate) order: Option<String>,
    #[serde(flatten)]
    pub(crate) caller: crate::client::CallerArgs,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RecordLessonArgs {
    #[serde(default)]
    pub(crate) topic: Option<String>,
    pub(crate) project_path: String,
    pub(crate) kind: String,
    pub(crate) text: String,
    #[serde(default)]
    pub(crate) source_turn_id: Option<i64>,
    #[serde(default)]
    pub(crate) agent_name: Option<String>,
    #[serde(default)]
    pub(crate) session_id: Option<String>,
    #[serde(default)]
    pub(crate) supersedes: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListLessonsArgs {
    pub(crate) project_path: String,
    #[serde(default)]
    pub(crate) order: Option<String>,
}

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    match name {
        TOOL_LIST_AGENT_DISPATCHES => {
            let args = parse_args_default::<ListAgentDispatchesArgs>(arguments)?;
            let mut q: Vec<(&str, String)> = Vec::new();
            if let Some(ref p) = args.project {
                q.push(("project", p.clone()));
            }
            if let Some(ref s) = args.since {
                q.push(("since", s.clone()));
            }
            if let Some(ref df) = args.date_from {
                q.push(("date_from", df.clone()));
            }
            if let Some(ref dt) = args.date_to {
                q.push(("date_to", dt.clone()));
            }
            if let Some(l) = args.limit {
                q.push(("limit", l.to_string()));
            }
            if let Some(o) = args.offset {
                q.push(("offset", o.to_string()));
            }
            if let Some(ref ord) = args.order {
                q.push(("order", ord.clone()));
            }
            let (value, note) = client
                .get_json_with_note("/api/agent-dispatches", &q)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success_with_note("agent_dispatches", value, note))
        }
        TOOL_RECALL_PROJECT_CONTEXT => {
            let args = parse_args_required::<RecallProjectContextArgs>(arguments)?;
            let value = client
                .recall_project_context(
                    &args.project_path,
                    args.query.as_deref(),
                    args.caller.scope(),
                )
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("project_context", value))
        }
        TOOL_RECALL_SIMILAR_TURNS => {
            let args = parse_args_required::<RecallSimilarTurnsArgs>(arguments)?;
            let value = client
                .recall_similar_turns(
                    &args.project_path,
                    &args.query,
                    args.limit,
                    args.order.as_deref(),
                    args.caller.scope(),
                )
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("similar_turns", value))
        }
        TOOL_RECORD_LESSON => {
            let args = parse_args_required::<RecordLessonArgs>(arguments)?;
            let value = client
                .record_lesson(
                    &args.project_path,
                    &args.kind,
                    &args.text,
                    crate::client::RecordLessonRequest {
                        topic: args.topic.as_deref(),
                        source_turn_id: args.source_turn_id,
                        agent_name: args.agent_name.as_deref(),
                        session_id: args.session_id.as_deref(),
                        supersedes: args.supersedes,
                    },
                )
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("lesson", value))
        }
        TOOL_LIST_LESSONS => {
            let args = parse_args_required::<ListLessonsArgs>(arguments)?;
            let value = client
                .list_lessons(&args.project_path, args.order.as_deref())
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("lessons", value))
        }
        TOOL_DELETE_LESSON => {
            let args = parse_args_required::<IdStringArgs>(arguments)?;
            let value = client
                .delete_lesson(&args.id)
                .await
                .map_err(McpError::into_rmcp)?;
            Ok(wrap_success("result", value))
        }
        other => Err(RmcpError::invalid_params(
            format!("unknown memory tool: {other}"),
            None,
        )),
    }
}

#[cfg(test)]
#[path = "memory_tests.rs"]
mod tests;
