pub const ELIGIBLE_BASE: &str = "FROM memory.operator_turns t
   JOIN memory.turn_facts f ON f.operator_turn_id = t.id
   JOIN requests.requests r ON r.id = t.start_request_id
  WHERE t.closed_at IS NOT NULL
    AND f.prompt_kind = 'genuine'
    AND f.session_class <> 'synthetic'
    AND r.claude_code_agent_id IS NULL
    AND f.next_state IN ('resolved', 'none_session_settled')";

pub const QUEUED: &str = "AND t.judge_queued_at IS NOT NULL";

pub const NOT_PERMANENT: &str = "AND t.judge_failure_reason IS DISTINCT FROM 'permanent'";

pub const NO_LIVE_VERDICT: &str = "AND NOT EXISTS (
        SELECT 1 FROM memory.turn_verdicts v
         WHERE v.operator_turn_id = t.id AND v.mode = 'live'
           AND v.bundle_id = $1 AND v.reaction IS NOT NULL
           AND v.next_genuine_request_id IS NOT DISTINCT FROM f.next_genuine_request_id)";

pub const BACKOFF_ELAPSED: &str =
    "AND (t.judge_next_attempt_at IS NULL OR t.judge_next_attempt_at <= now())";

pub const LEASE_FREE: &str =
    "AND (t.judge_claimed_until IS NULL OR t.judge_claimed_until <= now())";

pub fn eligible_without_verdict() -> String {
    format!("{ELIGIBLE_BASE} {NOT_PERMANENT} {NO_LIVE_VERDICT}")
}

pub fn queued_without_verdict() -> String {
    format!("{ELIGIBLE_BASE} {QUEUED} {NOT_PERMANENT} {NO_LIVE_VERDICT}")
}
