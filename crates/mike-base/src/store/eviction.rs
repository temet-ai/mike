use chrono::{Duration, Utc};

use super::RequestStore;
use super::models::{InterceptedRequest, RequestId};
use super::retention::RetentionPolicy;
use super::views::ByteBudgetEvictionOutcome;

pub fn failed_stream_blocks_bytes(req: &InterceptedRequest) -> u64 {
    req.failed_stream_blocks
        .iter()
        .map(|block| {
            (block.content.len()
                + block.signature.as_ref().map_or(0, String::len)
                + block.wire_tool_use_id.as_ref().map_or(0, String::len)
                + block.content_sha256.len()
                + block.block_type.len()) as u64
        })
        .sum()
}

fn entry_bytes(req: &InterceptedRequest) -> u64 {
    let request_body = req
        .request_body
        .as_ref()
        .map(|b| b.len() as u64)
        .unwrap_or(0);
    let response_body = req
        .response_body
        .as_ref()
        .map(|b| b.len() as u64)
        .unwrap_or(0);
    let filtered_request_body = req
        .filtered_request_body
        .as_ref()
        .map(|b| b.len() as u64)
        .unwrap_or(0);
    let tool_calls_bytes = req
        .tool_calls
        .iter()
        .map(|tc| (tc.tool_id.len() + tc.name.len() + tc.input_json.len()) as u64)
        .sum::<u64>();
    let tool_results_bytes = req
        .tool_results
        .iter()
        .map(|tr| tr.tool_use_id.len() as u64 + tr.content_bytes)
        .sum::<u64>();
    let tool_results_in_body_bytes = req
        .tool_results_in_body
        .iter()
        .map(|tr| tr.tool_use_id.len() as u64 + tr.content_bytes)
        .sum::<u64>();
    let system_prompt_blocks_bytes = req
        .system_prompt
        .blocks
        .as_ref()
        .map(|v| serde_json::to_vec(v).map(|b| b.len() as u64).unwrap_or(0))
        .unwrap_or(0);

    let failed_stream_blocks_bytes = failed_stream_blocks_bytes(req);

    request_body
        .saturating_add(failed_stream_blocks_bytes)
        .saturating_add(response_body)
        .saturating_add(filtered_request_body)
        .saturating_add(tool_calls_bytes)
        .saturating_add(tool_results_bytes)
        .saturating_add(tool_results_in_body_bytes)
        .saturating_add(system_prompt_blocks_bytes)
}

impl RequestStore {
    fn evict_oldest_with_reason(&mut self, reason: &'static str) -> Option<u64> {
        let now = Utc::now();
        let evict_pos = self.order.iter().enumerate().rev().find_map(|(i, id)| {
            self.entries.get(id).and_then(|req| {
                if !req.status.is_terminal() {
                    return None;
                }

                let meta = self
                    .session_meta
                    .get(&req.session_id)
                    .cloned()
                    .unwrap_or_default();
                if meta.is_pinned() {
                    return None;
                }

                if reason != "capacity_bytes" && reason != "cap_manager_shed" {
                    let ttl_std_duration = if let Some(ttl_str) = meta.ttl_override() {
                        RetentionPolicy::parse_duration(ttl_str)
                    } else {
                        self.retention.max_age
                    };
                    let ttl_chrono_duration = Duration::from_std(ttl_std_duration)
                        .unwrap_or_else(|_| Duration::seconds(3600));
                    let cutoff = now - ttl_chrono_duration;
                    if req.created_at > cutoff {
                        return None;
                    }
                }

                Some(i)
            })
        });

        let pos = evict_pos?;
        let id = self.order.remove(pos)?;
        let req = self.entries.remove(&id)?;
        let bytes_freed = entry_bytes(&req);
        let session_id = req.session_id.clone();
        let status = req.status.to_string();
        let created_at = req.created_at;
        drop(req);
        self.detach_from_session_index(&session_id, id);
        metrics::counter!(
            crate::metrics::COUNTER_STORE_EVICTIONS_TOTAL,
            "reason" => reason,
        )
        .increment(1);
        tracing::debug!(
            request_id = id.as_u64(),
            %session_id,
            %status,
            created_at = %created_at,
            bytes_freed,
            bytes_freed_human = %bytesize::ByteSize::b(bytes_freed),
            reason,
            "evicted in-memory request entry"
        );
        Some(bytes_freed)
    }

    pub fn evict_oldest(&mut self) {
        self.evict_oldest_with_reason("capacity");
    }

    pub fn shed_to_target(&mut self, target_bytes: u64) -> (u64, u64) {
        let mut bytes_freed: u64 = 0;
        let mut entries_evicted: u64 = 0;
        while bytes_freed < target_bytes {
            match self.evict_oldest_with_reason("cap_manager_shed") {
                Some(freed) => {
                    entries_evicted = entries_evicted.saturating_add(1);
                    bytes_freed = bytes_freed.saturating_add(freed);
                }
                None => break,
            }
        }
        (bytes_freed, entries_evicted)
    }

    pub fn terminal_entry_count(&self) -> usize {
        self.entries
            .values()
            .filter(|e| e.status.is_terminal())
            .count()
    }

    pub fn evict_to_byte_budget(&mut self) -> ByteBudgetEvictionOutcome {
        let budget = self.retention.max_store_bytes;
        if budget == 0 {
            let bytes = self.total_bytes();
            return ByteBudgetEvictionOutcome {
                entries_evicted: 0,
                bytes_freed: 0,
                bytes_remaining_after: bytes,
                budget,
            };
        }

        let mut bytes = self.total_bytes();
        let mut entries_evicted: u64 = 0;
        let mut bytes_freed: u64 = 0;

        while bytes > budget {
            match self.evict_oldest_with_reason("capacity_bytes") {
                Some(freed) => {
                    entries_evicted = entries_evicted.saturating_add(1);
                    bytes_freed = bytes_freed.saturating_add(freed);
                    bytes = bytes.saturating_sub(freed);
                }
                None => {
                    break;
                }
            }
        }

        ByteBudgetEvictionOutcome {
            entries_evicted,
            bytes_freed,
            bytes_remaining_after: bytes,
            budget,
        }
    }

    pub fn total_bytes(&self) -> u64 {
        self.entries.values().map(entry_bytes).sum()
    }

    pub fn remove_entry(&mut self, id: &RequestId) {
        if let Some(req) = self.entries.remove(id) {
            self.detach_from_session_index(&req.session_id, *id);
        }
        self.order.retain(|oid| oid != id);
    }
}
