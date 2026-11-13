-- no-transaction
-- Knowledge health counts recorded failures per model, capped; failures are rare, so this partial index keeps the count off the full state table.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_knowledge_embedding_state_failed ON memory.knowledge_embedding_state (model_key) WHERE last_error IS NOT NULL;
