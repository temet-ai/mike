use super::RequestStore;
use super::models;
use super::models::RequestId;
use super::views::AttachToolResultsOutcome;

impl RequestStore {
    pub(crate) fn attach_tool_results(
        &mut self,
        session_id: &str,
        results: Vec<models::ToolResult>,
    ) -> AttachToolResultsOutcome {
        let mut outcome = AttachToolResultsOutcome::default();
        if results.is_empty() {
            return outcome;
        }
        let bucket_ids: Vec<RequestId> = self
            .session_index
            .get(session_id)
            .map(|bucket| bucket.iter().copied().collect())
            .unwrap_or_default();
        if bucket_ids.is_empty() {
            outcome.db_fallback_candidates = results;
            return outcome;
        }

        let mut touched_set: std::collections::HashSet<u64> = std::collections::HashSet::new();

        for result in results {
            let target: Option<RequestId> = bucket_ids.iter().copied().find(|rid| {
                self.entries
                    .get(rid)
                    .map(|req| {
                        req.tool_calls
                            .iter()
                            .any(|tc| tc.tool_id == result.tool_use_id)
                    })
                    .unwrap_or(false)
            });

            let Some(target_id) = target else {
                outcome.db_fallback_candidates.push(result);
                continue;
            };

            if let Some(req) = self.entries.get_mut(&target_id) {
                let already_recorded = req.tool_results.iter().any(|existing| {
                    existing.tool_use_id == result.tool_use_id
                        && existing.content_sha256 == result.content_sha256
                });
                if !already_recorded {
                    req.tool_results.push(result);
                    if touched_set.insert(target_id.as_u64()) {
                        outcome.touched.push(target_id);
                    }
                }
            }
        }
        outcome
    }
}
