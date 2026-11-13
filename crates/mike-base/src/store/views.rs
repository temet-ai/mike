use chrono::{DateTime, Utc};
use std::collections::BTreeMap;

use super::models::{self, Diagnostic, DiagnosticCategory, RequestId};

#[derive(Debug, Clone, Default)]
pub struct DiagnosticStatsView {
    pub totals_by_category: BTreeMap<DiagnosticCategory, usize>,
    pub totals_by_analyzer: BTreeMap<String, usize>,
    pub last_24h: usize,
}

#[derive(Debug, Clone)]
pub struct FindingRequestRef {
    pub model: Option<String>,
    pub status: String,
    pub path: String,
    pub project: Option<String>,
    pub human_prompt: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct FindingView {
    pub request_id: RequestId,
    pub session_id: String,
    pub diagnostic: Diagnostic,
    pub request: FindingRequestRef,
}

#[derive(Debug, Clone, Default)]
pub struct MemoryBreakdownView {
    pub entry_count: u64,
    pub request_body_bytes: u64,
    pub response_body_bytes: u64,
    pub filtered_request_body_bytes: u64,
    pub tool_calls_bytes: u64,
    pub tool_results_bytes: u64,
    pub tool_results_in_body_bytes: u64,
    pub system_prompt_blocks_bytes: u64,
    pub failed_stream_blocks_bytes: u64,
    pub entries_total_bytes: u64,

    pub session_index_keys: u64,
    pub composable_session_index_keys: u64,
    pub session_meta_keys: u64,
    pub in_flight_count: u64,

    pub process_rss_bytes: Option<u64>,

    pub max_entries: u64,
    pub max_store_bytes: u64,
    pub over_byte_budget: bool,

    pub top_entries: Vec<MemoryEntryRow>,
}

#[derive(Debug, Clone)]
pub struct MemoryEntryRow {
    pub id: RequestId,
    pub session_id: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub request_body_bytes: u64,
    pub response_body_bytes: u64,
    pub filtered_request_body_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ByteBudgetEvictionOutcome {
    pub entries_evicted: u64,
    pub bytes_freed: u64,
    pub bytes_remaining_after: u64,
    pub budget: u64,
}

impl ByteBudgetEvictionOutcome {
    pub fn over_budget(&self) -> bool {
        self.budget > 0 && self.bytes_remaining_after > self.budget
    }
}

#[derive(Debug, Default)]
pub struct AttachToolResultsOutcome {
    pub touched: Vec<RequestId>,
    pub db_fallback_candidates: Vec<models::ToolResult>,
}
