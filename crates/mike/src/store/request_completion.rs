use std::sync::Arc;

use bytes::Bytes;
use chrono::Utc;
use tokio::sync::RwLock;

use super::RequestStore;
use super::models::{RequestId, RequestStatus};
use super::updater::run_analysis_and_notify;
use super::views::ByteBudgetEvictionOutcome;

pub(crate) struct NonStreamingCompletion {
    pub(crate) request_id: RequestId,
    pub(crate) response_status: u16,
    pub(crate) response_headers: Vec<(String, String)>,
    pub(crate) response_body: Bytes,
    pub(crate) response_text: String,
    pub(crate) upstream_request_id: Option<String>,
}

impl RequestStore {
    pub(crate) async fn complete_non_streaming(
        store: &Arc<RwLock<RequestStore>>,
        completion: NonStreamingCompletion,
    ) {
        let NonStreamingCompletion {
            request_id,
            response_status,
            response_headers,
            response_body,
            response_text,
            upstream_request_id,
        } = completion;
        let (usage, stop_reason) =
            crate::extract::response::extract_completion_from_body(&response_body);

        {
            let mut s = store.write().await;
            s.update(&request_id, |req| {
                req.status = RequestStatus::Complete;
                if usage.is_some() {
                    req.token_usage = usage;
                }
                if stop_reason.is_some() {
                    req.stop_reason = stop_reason;
                }
                req.response_status = Some(response_status);
                req.response_headers = Some(response_headers);
                req.response_body = Some(response_body);
                req.completed_at = Some(Utc::now());
                if let Some(ref id) = upstream_request_id {
                    req.upstream_request_id = Some(id.clone());
                }
            });
        }

        {
            let mut s = store.write().await;
            let outcome = s.evict_to_byte_budget();
            Self::log_non_streaming_eviction_outcome(request_id, &outcome);
        }

        run_analysis_and_notify(store, request_id, &response_text).await;

        Self::snapshot_terminal(store, request_id).await;
    }

    fn log_non_streaming_eviction_outcome(
        request_id: RequestId,
        outcome: &ByteBudgetEvictionOutcome,
    ) {
        if outcome.over_budget() {
            Self::warn_non_streaming_still_over_budget(request_id, outcome);
        } else if outcome.entries_evicted > 0 {
            Self::info_non_streaming_evicted_to_budget(request_id, outcome);
        }
    }

    fn warn_non_streaming_still_over_budget(
        request_id: RequestId,
        outcome: &ByteBudgetEvictionOutcome,
    ) {
        tracing::warn!(
            terminal_request_id = request_id.as_u64(),
            entries_evicted = outcome.entries_evicted,
            bytes_freed = outcome.bytes_freed,
            bytes_remaining_after = outcome.bytes_remaining_after,
            bytes_remaining_after_human = %bytesize::ByteSize::b(outcome.bytes_remaining_after),
            budget = outcome.budget,
            budget_human = %bytesize::ByteSize::b(outcome.budget),
            "in-memory store still over byte budget after non-streaming terminal eviction — every remaining entry is non-terminal or pinned"
        );
    }

    fn info_non_streaming_evicted_to_budget(
        request_id: RequestId,
        outcome: &ByteBudgetEvictionOutcome,
    ) {
        tracing::info!(
            terminal_request_id = request_id.as_u64(),
            entries_evicted = outcome.entries_evicted,
            bytes_freed = outcome.bytes_freed,
            bytes_freed_human = %bytesize::ByteSize::b(outcome.bytes_freed),
            bytes_remaining_after = outcome.bytes_remaining_after,
            budget = outcome.budget,
            "evicted to honour byte budget on non-streaming terminal"
        );
    }
}
