-- no-transaction
-- clear_transient_judge_backoff runs per verdict and scanned the table for its handful of rows.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_operator_turns_transient_backoff ON memory.operator_turns (judge_next_attempt_at) WHERE judge_failure_reason = 'transient_exhausted';
