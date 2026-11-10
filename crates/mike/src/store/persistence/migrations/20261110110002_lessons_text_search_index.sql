-- no-transaction
-- The knowledge search computes to_tsvector('english', text) for every lesson of the project; an expression index with
-- the same expression lets its lesson branch ask the index for the matching lessons. CONCURRENTLY does not block writers.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_lessons_text_tsv
    ON memory.lessons USING gin (to_tsvector('english', text));
