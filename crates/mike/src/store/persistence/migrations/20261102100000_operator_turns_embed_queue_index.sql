-- no-transaction
-- The embed queue (fetch_unembedded_turn_ids and its count) filters operator-opened turns; idx_operator_turns_pending_embed also held every closed non-operator turn, which is never embedded, so the planner scanned the table instead.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_operator_turns_embed_queue ON memory.operator_turns (closed_at) WHERE closed_at IS NOT NULL AND embedded_at IS NULL AND (opened_by IS NULL OR opened_by = 'operator');
