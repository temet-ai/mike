use std::collections::HashSet;

use crate::steerer::verifier::classify_ask_kind;
use crate::store::persistence::ask_ledger_backfill::{BuilderLedgerRow, LedgerCandidateRow};

pub fn plan_builder_rows(candidates: &[LedgerCandidateRow]) -> Vec<BuilderLedgerRow> {
    let mut seen_sessions: HashSet<i64> = HashSet::new();
    let mut rows = Vec::new();
    for candidate in candidates {
        let content = candidate.user_prompt.trim();
        if content.is_empty() {
            continue;
        }
        let is_first = !candidate.has_earlier && !seen_sessions.contains(&candidate.session_id);
        seen_sessions.insert(candidate.session_id);
        rows.push(BuilderLedgerRow {
            session_id: candidate.session_id,
            operator_turn_id: candidate.turn_id,
            request_id: candidate.start_request_id,
            kind: classify_ask_kind(is_first, content).to_owned(),
            content: content.to_owned(),
            created_at: candidate.created_at,
        });
    }
    rows
}
