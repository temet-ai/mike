-- no-transaction
-- Replaced by idx_operator_turns_embed_queue; one scan since statistics began.
DROP INDEX CONCURRENTLY IF EXISTS memory.idx_operator_turns_pending_embed;
