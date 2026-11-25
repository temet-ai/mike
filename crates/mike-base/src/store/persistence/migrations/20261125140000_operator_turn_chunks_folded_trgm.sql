-- no-transaction
-- Content search folds each chunk once with lower(... COLLATE "pg_c_utf8") and compares every word with LIKE.
-- ILIKE folds the chunk again for every word, with the slower libc lower().
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_otc_folded_text_trgm ON memory.operator_turn_chunks USING gin (lower(chunk_text COLLATE "pg_c_utf8") gin_trgm_ops);
