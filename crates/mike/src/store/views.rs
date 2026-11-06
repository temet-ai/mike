use chrono::{DateTime, Utc};
use std::collections::BTreeMap;

use super::models::{self, Diagnostic, DiagnosticCategory, RequestId};

#[derive(Debug, Clone, Default)]
pub(crate) struct DiagnosticStatsView {
    pub(crate) totals_by_category: BTreeMap<DiagnosticCategory, usize>,
    pub(crate) totals_by_analyzer: BTreeMap<String, usize>,
    pub(crate) last_24h: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct FindingRequestRef {
    pub(crate) model: Option<String>,
    pub(crate) status: String,
    pub(crate) path: String,
    pub(crate) project: Option<String>,
    pub(crate) human_prompt: String,
    pub(crate) created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub(crate) struct FindingView {
    pub(crate) request_id: RequestId,
    pub(crate) session_id: String,
    pub(crate) diagnostic: Diagnostic,
    pub(crate) request: FindingRequestRef,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct MemoryBreakdownView {
    pub(crate) entry_count: u64,
    pub(crate) request_body_bytes: u64,
    pub(crate) response_body_bytes: u64,
    pub(crate) filtered_request_body_bytes: u64,
    pub(crate) tool_calls_bytes: u64,
    pub(crate) tool_results_bytes: u64,
    pub(crate) tool_results_in_body_bytes: u64,
    pub(crate) system_prompt_blocks_bytes: u64,
    pub(crate) failed_stream_blocks_bytes: u64,
    pub(crate) entries_total_bytes: u64,

    pub(crate) session_index_keys: u64,
    pub(crate) composable_session_index_keys: u64,
    pub(crate) session_meta_keys: u64,
    pub(crate) in_flight_count: u64,

    pub(crate) process_rss_bytes: Option<u64>,

    pub(crate) max_entries: u64,
    pub(crate) max_store_bytes: u64,
    pub(crate) over_byte_budget: bool,

    pub(crate) top_entries: Vec<MemoryEntryRow>,
}

#[derive(Debug, Clone)]
pub(crate) struct MemoryEntryRow {
    pub(crate) id: RequestId,
    pub(crate) session_id: String,
    pub(crate) status: String,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) request_body_bytes: u64,
    pub(crate) response_body_bytes: u64,
    pub(crate) filtered_request_body_bytes: u64,
    pub(crate) total_bytes: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ByteBudgetEvictionOutcome {
    pub(crate) entries_evicted: u64,
    pub(crate) bytes_freed: u64,
    pub(crate) bytes_remaining_after: u64,
    pub(crate) budget: u64,
}

impl ByteBudgetEvictionOutcome {
    pub(crate) fn over_budget(&self) -> bool {
        self.budget > 0 && self.bytes_remaining_after > self.budget
    }
}

#[derive(Debug, Default)]
pub(crate) struct AttachToolResultsOutcome {
    pub(crate) touched: Vec<RequestId>,
    pub(crate) db_fallback_candidates: Vec<models::ToolResult>,
}
