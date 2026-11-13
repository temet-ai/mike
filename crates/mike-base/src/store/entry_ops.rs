use chrono::{DateTime, Duration, Utc};
use std::collections::VecDeque;

use super::RequestStore;
use super::models::{InterceptedRequest, RequestId, RequestStatus};
use super::retention::RetentionPolicy;
use super::views::ByteBudgetEvictionOutcome;

impl RequestStore {
    pub fn insert(&mut self, req: InterceptedRequest) -> RequestId {
        let id = req.id;
        if self.entries.len() >= self.retention.max_entries {
            self.evict_oldest();
        }

        self.record_new_request_membership(id, &req);
        self.entries.insert(id, req);

        let outcome = self.evict_to_byte_budget();
        Self::log_insert_eviction_outcome(&outcome);

        id
    }

    fn record_new_request_membership(&mut self, id: RequestId, req: &InterceptedRequest) {
        let session_id = req.session_id.clone();
        self.order.push_front(id);
        self.session_index
            .entry(session_id.clone())
            .or_default()
            .push_front(id);
        if req.turn_origin == "primary_first" {
            self.composable_session_index
                .entry(session_id.clone())
                .or_default();
        }
        self.session_meta.entry(session_id).or_default();
    }

    fn log_insert_eviction_outcome(outcome: &ByteBudgetEvictionOutcome) {
        if outcome.over_budget() {
            Self::warn_insert_still_over_budget(outcome);
        } else if outcome.entries_evicted > 0 {
            Self::info_insert_evicted_to_budget(outcome);
        }
    }

    fn warn_insert_still_over_budget(outcome: &ByteBudgetEvictionOutcome) {
        tracing::warn!(
            entries_evicted = outcome.entries_evicted,
            bytes_freed = outcome.bytes_freed,
            bytes_remaining_after = outcome.bytes_remaining_after,
            bytes_remaining_after_human = %bytesize::ByteSize::b(outcome.bytes_remaining_after),
            budget = outcome.budget,
            budget_human = %bytesize::ByteSize::b(outcome.budget),
            "in-memory store still over byte budget after on-insert eviction — every remaining entry is non-terminal or pinned"
        );
    }

    fn info_insert_evicted_to_budget(outcome: &ByteBudgetEvictionOutcome) {
        tracing::info!(
            entries_evicted = outcome.entries_evicted,
            bytes_freed = outcome.bytes_freed,
            bytes_freed_human = %bytesize::ByteSize::b(outcome.bytes_freed),
            bytes_remaining_after = outcome.bytes_remaining_after,
            budget = outcome.budget,
            "evicted to honour byte budget on insert"
        );
    }

    pub fn project_for_session(&self, session_id: &str) -> Option<String> {
        self.session_index.get(session_id).and_then(|bucket| {
            bucket
                .iter()
                .find_map(|rid| self.entries.get(rid).and_then(|r| r.project.clone()))
        })
    }

    pub fn session_is_new(&self, session_id: &str) -> bool {
        self.session_index
            .get(session_id)
            .is_none_or(VecDeque::is_empty)
    }

    pub fn get(&self, id: &RequestId) -> Option<&InterceptedRequest> {
        self.entries.get(id)
    }

    pub fn recent(&self, limit: usize) -> Vec<&InterceptedRequest> {
        self.order
            .iter()
            .take(limit)
            .filter_map(|id| self.entries.get(id))
            .collect()
    }

    pub fn update(&mut self, id: &RequestId, f: impl FnOnce(&mut InterceptedRequest)) -> bool {
        match self.entries.get_mut(id) {
            Some(req) => {
                f(req);
                true
            }
            None => false,
        }
    }

    pub fn mark_aborted_if_non_terminal(&mut self, id: RequestId, reason: &str) {
        if let Some(req) = self.entries.get_mut(&id)
            && !req.status.is_terminal()
        {
            req.status = RequestStatus::Error(reason.to_string());
            req.completed_at = Some(Utc::now());
        }
    }

    pub fn prune_before(&mut self, cutoff: DateTime<Utc>) -> usize {
        let ids_to_remove: Vec<RequestId> = self
            .entries
            .iter()
            .filter(|(_, req)| {
                if !req.status.is_terminal() {
                    return false;
                }

                let meta = self
                    .session_meta
                    .get(&req.session_id)
                    .cloned()
                    .unwrap_or_default();
                if meta.is_pinned() {
                    return false;
                }

                let effective_cutoff = if let Some(ttl_str) = meta.ttl_override() {
                    let ttl_std_duration = RetentionPolicy::parse_duration(ttl_str);
                    let ttl_chrono_duration = Duration::from_std(ttl_std_duration)
                        .unwrap_or_else(|_| Duration::seconds(3600));
                    Utc::now() - ttl_chrono_duration
                } else {
                    cutoff
                };

                req.created_at < effective_cutoff
            })
            .map(|(id, _)| *id)
            .collect();

        let count = ids_to_remove.len();
        for id in &ids_to_remove {
            self.remove_entry(id);
        }
        count
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}
