-- no-transaction
-- Knowledge health counts embedded sources per model, capped; after a model change no row matches, and without this index the capped count read the whole state table.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_knowledge_embedding_state_ready ON memory.knowledge_embedding_state (model_key) WHERE completed_at IS NOT NULL;
