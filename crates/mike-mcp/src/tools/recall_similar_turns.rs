use std::borrow::Cow;

use rmcp::model::{CallToolResult, ErrorData as RmcpError, Tool};
use serde_json::{Map, Value, json};

use super::memory::{RecallSimilarTurnsArgs, TOOL_RECALL_SIMILAR_TURNS};
use super::{object_schema, parse_args_required, wrap_success};
use crate::client::InspectClient;
use crate::error::McpError;

pub(super) async fn call(
    client: &InspectClient,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    let args = parse_args_required::<RecallSimilarTurnsArgs>(arguments)?;
    let value = client
        .recall_similar_turns(
            &args.project_path,
            &args.query,
            args.limit,
            crate::client::RecallShape {
                order: args.order.as_deref(),
                detail: args.detail.as_deref(),
            },
            args.caller.scope(),
        )
        .await
        .map_err(McpError::into_rmcp)?;
    Ok(wrap_success("similar_turns", value))
}

pub(super) fn tool() -> Tool {
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
            (
                "detail",
                json!({"type": "string", "enum": ["best", "full"], "description": "best (default) returns the first turn with its text and every later turn as a pointer; full returns every turn in full."}),
            ),
        ],
        &["project_path", "query"],
    );
    Tool::new(
        Cow::Borrowed(TOOL_RECALL_SIMILAR_TURNS),
        Cow::Borrowed(
            "Find the operator turns of a project that resemble a query: by embedding similarity \
             when embeddings are available, otherwise by hybrid text and semantic search. Returns \
             `{turns, retrieval_path, degraded, pending_turns?, rerank, more?}`. The first turn is \
             `{turn_id, reference, session_id, created_at, user_prompt, final_assistant_text, \
             outcome}`; each later turn is a pointer `{turn_id, reference, \
             created_at, session_id, prompt}` (the first 100 characters of the prompt) to read \
             with read_project_knowledge, and `more` says how to get every turn in full \
             (`detail=full`). `rerank` is `{state: reranked|loading|off|unavailable, \
             ready_in_s, detail?}`: when state is reranked, a reranker model has reordered the \
             turns (the first turn then has `rerank_score`); otherwise `detail` says why they are \
             in plain order. `degraded: true` means embeddings were unavailable and full-text \
             search served the result, so ranking is weaker. `pending_turns`, when present, \
             counts turns without embeddings; a non-zero value means ranking improves once the \
             back-fill completes. For questions about the latest or current work, pass \
             `order=recent`.",
        ),
        schema,
    )
}
