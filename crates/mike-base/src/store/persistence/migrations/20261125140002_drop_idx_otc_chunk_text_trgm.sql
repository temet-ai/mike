-- no-transaction
-- Content search, its only reader, reads idx_otc_folded_text_trgm.
DROP INDEX CONCURRENTLY IF EXISTS memory.idx_otc_chunk_text_trgm;
