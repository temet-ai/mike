-- no-transaction
-- The knowledge search matched document revisions by scanning every row of memory.knowledge_revisions and reading its
-- text vector. search_tsv is stored, so a GIN index over it reads the vectors, not the bodies. CONCURRENTLY does not
-- block writers. A migration file with CREATE INDEX CONCURRENTLY holds that one statement, because several statements
-- sent together run as one transaction.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_knowledge_revisions_search_tsv
    ON memory.knowledge_revisions USING gin (search_tsv);
