-- no-transaction
-- Open turns: count_open_unembedded_turns, the stale-turn closer and the per-session open-turn lookups.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_operator_turns_open ON memory.operator_turns (session_id) WHERE closed_at IS NULL;
