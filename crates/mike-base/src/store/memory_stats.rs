use std::cmp::Reverse;
use std::collections::BinaryHeap;

use super::RequestStore;
use super::models::{InterceptedRequest, RequestId};
use super::views::{DiagnosticStatsView, MemoryBreakdownView, MemoryEntryRow};

struct EntryByteBreakdown {
    request_body: u64,
    response_body: u64,
    filtered_request_body: u64,
    tool_calls: u64,
    tool_results: u64,
    tool_results_in_body: u64,
    system_prompt_blocks: u64,
    failed_stream_blocks: u64,
}

impl EntryByteBreakdown {
    fn compute(req: &InterceptedRequest) -> Self {
        Self {
            failed_stream_blocks: super::eviction::failed_stream_blocks_bytes(req),
            request_body: req
                .request_body
                .as_ref()
                .map(|b| b.len() as u64)
                .unwrap_or(0),
            response_body: req
                .response_body
                .as_ref()
                .map(|b| b.len() as u64)
                .unwrap_or(0),
            filtered_request_body: req
                .filtered_request_body
                .as_ref()
                .map(|b| b.len() as u64)
                .unwrap_or(0),
            tool_calls: serde_json::to_vec(&req.tool_calls)
                .map(|v| v.len() as u64)
                .unwrap_or(0),
            tool_results: serde_json::to_vec(&req.tool_results)
                .map(|v| v.len() as u64)
                .unwrap_or(0),
            tool_results_in_body: serde_json::to_vec(&req.tool_results_in_body)
                .map(|v| v.len() as u64)
                .unwrap_or(0),
            system_prompt_blocks: req
                .system_prompt
                .blocks
                .as_ref()
                .map(|v| serde_json::to_vec(v).map(|b| b.len() as u64).unwrap_or(0))
                .unwrap_or(0),
        }
    }

    fn total(&self) -> u64 {
        self.request_body
            .saturating_add(self.response_body)
            .saturating_add(self.filtered_request_body)
            .saturating_add(self.tool_calls)
            .saturating_add(self.tool_results)
            .saturating_add(self.tool_results_in_body)
            .saturating_add(self.system_prompt_blocks)
            .saturating_add(self.failed_stream_blocks)
    }
}

impl RequestStore {
    pub fn memory_breakdown(&self, top_n: usize) -> MemoryBreakdownView {
        let mut view = MemoryBreakdownView {
            entry_count: self.entries.len() as u64,
            session_index_keys: self.session_index.len() as u64,
            composable_session_index_keys: self.composable_session_index.len() as u64,
            session_meta_keys: self.session_meta.len() as u64,
            max_entries: self.retention.max_entries as u64,
            max_store_bytes: self.retention.max_store_bytes,
            ..MemoryBreakdownView::default()
        };

        let mut heap: BinaryHeap<Reverse<(u64, u64)>> = BinaryHeap::with_capacity(top_n);

        for req in self.entries.values() {
            let b = EntryByteBreakdown::compute(req);

            view.request_body_bytes = view.request_body_bytes.saturating_add(b.request_body);
            view.response_body_bytes = view.response_body_bytes.saturating_add(b.response_body);
            view.filtered_request_body_bytes = view
                .filtered_request_body_bytes
                .saturating_add(b.filtered_request_body);
            view.tool_calls_bytes = view.tool_calls_bytes.saturating_add(b.tool_calls);
            view.tool_results_bytes = view.tool_results_bytes.saturating_add(b.tool_results);
            view.tool_results_in_body_bytes = view
                .tool_results_in_body_bytes
                .saturating_add(b.tool_results_in_body);
            view.system_prompt_blocks_bytes = view
                .system_prompt_blocks_bytes
                .saturating_add(b.system_prompt_blocks);
            view.failed_stream_blocks_bytes = view
                .failed_stream_blocks_bytes
                .saturating_add(b.failed_stream_blocks);

            let total = b.total();
            if top_n > 0 {
                if heap.len() < top_n {
                    heap.push(Reverse((total, req.id.as_u64())));
                } else if let Some(Reverse((smallest, _))) = heap.peek()
                    && total > *smallest
                {
                    heap.pop();
                    heap.push(Reverse((total, req.id.as_u64())));
                }
            }
        }

        view.entries_total_bytes = view
            .request_body_bytes
            .saturating_add(view.response_body_bytes)
            .saturating_add(view.filtered_request_body_bytes)
            .saturating_add(view.tool_calls_bytes)
            .saturating_add(view.tool_results_bytes)
            .saturating_add(view.tool_results_in_body_bytes)
            .saturating_add(view.system_prompt_blocks_bytes)
            .saturating_add(view.failed_stream_blocks_bytes);

        view.over_byte_budget =
            view.max_store_bytes > 0 && view.entries_total_bytes > view.max_store_bytes;

        view.top_entries = self.build_top_entries(heap);
        view
    }

    fn build_top_entries(&self, heap: BinaryHeap<Reverse<(u64, u64)>>) -> Vec<MemoryEntryRow> {
        let mut top_entries: Vec<MemoryEntryRow> = heap
            .into_sorted_vec()
            .into_iter()
            .rev()
            .filter_map(|Reverse((total, raw_id))| {
                let id = RequestId::from_raw(raw_id);
                let req = self.entries.get(&id)?;
                let b = EntryByteBreakdown::compute(req);
                Some(MemoryEntryRow {
                    id,
                    session_id: req.session_id.clone(),
                    status: req.status.to_string(),
                    created_at: req.created_at,
                    request_body_bytes: b.request_body,
                    response_body_bytes: b.response_body,
                    filtered_request_body_bytes: b.filtered_request_body,
                    total_bytes: total,
                })
            })
            .collect();
        top_entries.sort_by_key(|row| std::cmp::Reverse(row.total_bytes));
        top_entries
    }

    pub fn diagnostic_stats(&self, now: chrono::DateTime<chrono::Utc>) -> DiagnosticStatsView {
        use chrono::Duration;
        let cutoff_24h = now - Duration::hours(24);
        let mut view = DiagnosticStatsView::default();
        for req in self.entries.values() {
            for diag in &req.diagnostics {
                *view
                    .totals_by_category
                    .entry(diag.category.clone())
                    .or_insert(0) += 1;
                *view
                    .totals_by_analyzer
                    .entry(diag.analyzer.clone())
                    .or_insert(0) += 1;
                if diag.created_at >= cutoff_24h {
                    view.last_24h += 1;
                }
            }
        }
        view
    }
}
