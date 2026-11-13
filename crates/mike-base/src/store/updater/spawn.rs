use std::sync::Arc;

use tokio::sync::{RwLock, mpsc};
use tokio_util::task::TaskTracker;

use super::apply;
use super::block_writer::{
    BlockAccumulator, BlockKind, BlockWriteTarget, DEFAULT_SUBAGENT_TYPE, extract_subagent_type,
    write_agent_dispatch_row, write_tool_use_row,
};
use super::response_hook::{CompletedResponse, ResponseHook};
use crate::in_flight::InFlightRegistry;
use crate::store::RequestStore;
use crate::store::models::{ProxyEvent, RequestId};
use crate::store::persistence::PersistHandle;

pub struct StoreUpdaterSpawnArgs {
    pub store: Arc<RwLock<RequestStore>>,
    pub event_rx: mpsc::Receiver<ProxyEvent>,
    pub request_id: RequestId,
    pub in_flight: Arc<InFlightRegistry>,
    pub persist: Option<PersistHandle>,
    pub response_hook: Option<Arc<dyn ResponseHook>>,
}

pub struct UpdaterLoopCtx<'a> {
    pub block_target: BlockWriteTarget<'a>,
    pub request_id_i64: i64,
    request_created_at: chrono::DateTime<chrono::Utc>,
    pub response_hook: Option<&'a dyn ResponseHook>,
}

pub fn spawn_store_updater(args: StoreUpdaterSpawnArgs, tracker: &TaskTracker) {
    crate::observability::spans::spawn_tracked(tracker, async move {
        let StoreUpdaterSpawnArgs {
            store,
            mut event_rx,
            request_id,
            in_flight,
            persist,
            response_hook,
        } = args;

        let mut saw_terminal = false;
        let request_id_i64 = i64::try_from(request_id.as_u64()).unwrap_or(i64::MAX);
        let request_created_at = {
            let s = store.read().await;
            s.get(&request_id)
                .map(|r| r.created_at)
                .unwrap_or_else(chrono::Utc::now)
        };
        let ctx = UpdaterLoopCtx {
            block_target: BlockWriteTarget {
                persist: &persist,
                store: &store,
                request_id,
            },
            request_id_i64,
            request_created_at,
            response_hook: response_hook.as_deref(),
        };
        let mut blocks = BlockAccumulator::default();
        let mut text_for_analysis = String::new();

        while let Some(event) = event_rx.recv().await {
            let is_terminal =
                process_one_event(&ctx, &mut blocks, &mut text_for_analysis, event).await;
            if is_terminal {
                handle_terminal(&ctx, &mut blocks, &text_for_analysis).await;
                saw_terminal = true;
                break;
            }
        }

        if !saw_terminal {
            handle_non_terminal_exit(&ctx, &mut blocks).await;
        }
        in_flight.remove(request_id);

        log_stream_completion(&store, request_id).await;
    });
}

async fn process_one_event(
    ctx: &UpdaterLoopCtx<'_>,
    blocks: &mut BlockAccumulator,
    text_for_analysis: &mut String,
    event: ProxyEvent,
) -> bool {
    if let ProxyEvent::TextDelta { text, .. } = &event {
        text_for_analysis.push_str(text);
    }
    match &event {
        ProxyEvent::TextDelta { text, .. } => {
            blocks.switch_to(BlockKind::Text, &ctx.block_target).await;
            blocks.push_text(text);
        }
        ProxyEvent::ThinkingDelta { thinking, .. } => {
            blocks
                .switch_to(BlockKind::Thinking, &ctx.block_target)
                .await;
            blocks.push_text(thinking);
        }
        ProxyEvent::SignatureDelta { signature, .. } => {
            blocks
                .switch_to(BlockKind::Thinking, &ctx.block_target)
                .await;
            blocks.set_signature(signature.clone());
        }
        _ => {}
    }

    let tool_end = capture_completing_tool_call(ctx.block_target.store, &event).await;

    let is_terminal = apply::apply_event(ctx.block_target.store, &event).await;

    if let Some((tc, ord)) = tool_end {
        write_completed_tool_call(ctx, blocks, &tc, ord).await;
    }

    is_terminal
}

async fn capture_completing_tool_call(
    store: &Arc<RwLock<RequestStore>>,
    event: &ProxyEvent,
) -> Option<(crate::types::ToolCall, i32)> {
    let ProxyEvent::ToolUseEnd { id } = event else {
        return None;
    };
    let s = store.read().await;
    s.get(id).and_then(|r| {
        let ord = r.tool_calls.len() as i32 - 1;
        r.tool_calls
            .last()
            .filter(|tc| !tc.complete)
            .map(|tc| (tc.clone(), ord))
    })
}

async fn write_completed_tool_call(
    ctx: &UpdaterLoopCtx<'_>,
    blocks: &mut BlockAccumulator,
    tc: &crate::types::ToolCall,
    ord: i32,
) {
    blocks
        .write_tool_use_block(&ctx.block_target, &tc.tool_id)
        .await;
    write_tool_use_row(
        ctx.block_target.persist,
        ctx.request_id_i64,
        ctx.request_created_at,
        ord,
        tc,
    )
    .await;
    if tc.name == "Agent" {
        let subagent_name = extract_subagent_type(&tc.input_json)
            .unwrap_or_else(|| DEFAULT_SUBAGENT_TYPE.to_string());
        write_agent_dispatch_row(ctx.block_target.persist, &tc.tool_id, &subagent_name).await;
    }
}

async fn handle_terminal(
    ctx: &UpdaterLoopCtx<'_>,
    blocks: &mut BlockAccumulator,
    text_for_analysis: &str,
) {
    blocks.flush(&ctx.block_target).await;
    {
        let mut s = ctx.block_target.store.write().await;
        s.update(&ctx.block_target.request_id, |req| {
            req.expected_stream_block_count = blocks.next_seq_ord;
        });
    }
    apply::run_analysis_and_notify(
        ctx.block_target.store,
        ctx.block_target.request_id,
        text_for_analysis,
    )
    .await;
    let scope = super::response_scope::read_response_scope(ctx).await;
    RequestStore::snapshot_terminal(ctx.block_target.store, ctx.block_target.request_id).await;

    if let (Some(hook), Some(persist)) = (ctx.response_hook, ctx.block_target.persist.as_ref()) {
        hook.on_response(CompletedResponse {
            persist,
            request_id: ctx.block_target.request_id,
            request_id_i64: ctx.request_id_i64,
            project_path: &scope.project_path,
            session_uuid: &scope.session_uuid,
            text: text_for_analysis,
        })
        .await;
    }

    evict_to_byte_budget_and_log(ctx.block_target.store, ctx.block_target.request_id).await;
}

async fn evict_to_byte_budget_and_log(store: &Arc<RwLock<RequestStore>>, request_id: RequestId) {
    let mut s = store.write().await;
    let outcome = s.evict_to_byte_budget();
    if outcome.over_budget() {
        warn_terminal_still_over_budget(request_id, &outcome);
    } else if outcome.entries_evicted > 0 {
        info_terminal_evicted_to_budget(request_id, &outcome);
    }
}

fn warn_terminal_still_over_budget(
    request_id: RequestId,
    outcome: &crate::store::views::ByteBudgetEvictionOutcome,
) {
    tracing::warn!(
        terminal_request_id = request_id.as_u64(),
        entries_evicted = outcome.entries_evicted,
        bytes_freed = outcome.bytes_freed,
        bytes_remaining_after = outcome.bytes_remaining_after,
        bytes_remaining_after_human = %bytesize::ByteSize::b(outcome.bytes_remaining_after),
        budget = outcome.budget,
        budget_human = %bytesize::ByteSize::b(outcome.budget),
        "in-memory store still over byte budget after terminal-time eviction — every remaining entry is non-terminal or pinned"
    );
}

fn info_terminal_evicted_to_budget(
    request_id: RequestId,
    outcome: &crate::store::views::ByteBudgetEvictionOutcome,
) {
    tracing::info!(
        terminal_request_id = request_id.as_u64(),
        entries_evicted = outcome.entries_evicted,
        bytes_freed = outcome.bytes_freed,
        bytes_freed_human = %bytesize::ByteSize::b(outcome.bytes_freed),
        bytes_remaining_after = outcome.bytes_remaining_after,
        budget = outcome.budget,
        "evicted to honour byte budget on terminal"
    );
}

async fn handle_non_terminal_exit(ctx: &UpdaterLoopCtx<'_>, blocks: &mut BlockAccumulator) {
    blocks.flush(&ctx.block_target).await;
    {
        let mut s = ctx.block_target.store.write().await;
        s.mark_aborted_if_non_terminal(
            ctx.block_target.request_id,
            "stream closed without terminal event",
        );
    }
    crate::store::RequestStore::close_aborted_request_in_db(
        ctx.block_target.store,
        ctx.block_target.request_id,
    )
    .await;
}

async fn log_stream_completion(store: &Arc<RwLock<RequestStore>>, request_id: RequestId) {
    let s = store.read().await;
    match s.get(&request_id) {
        Some(req) => log_stream_complete_info(request_id, req),
        None => {
            tracing::debug!(request_id = %request_id, "store updater finished (request not found)");
        }
    }
}

fn log_stream_complete_info(request_id: RequestId, req: &crate::store::models::InterceptedRequest) {
    let duration_ms = (chrono::Utc::now() - req.created_at)
        .num_milliseconds()
        .max(0) as u64;
    let input_tokens = req
        .token_usage
        .as_ref()
        .map(|u| u.input_tokens)
        .unwrap_or(0);
    let output_tokens = req
        .token_usage
        .as_ref()
        .map(|u| u.output_tokens)
        .unwrap_or(0);
    let response_status = req.response_status.unwrap_or(0);
    tracing::info!(
        request_id = %request_id,
        status = response_status,
        duration_ms = duration_ms,
        input_tokens = input_tokens,
        output_tokens = output_tokens,
        stop_reason = ?req.stop_reason,
        "stream complete"
    );
}
