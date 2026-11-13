-- no-transaction
-- The live judge window (live_window_backlog, claim_judge_turns, the final-turns projection) reads turns closed in the last days.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_operator_turns_closed_at ON memory.operator_turns (closed_at) WHERE closed_at IS NOT NULL;
