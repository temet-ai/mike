-- no-transaction
-- The knowledge search looks up each query word among one project's operator turns. idx_operator_turns_search_tsv
-- holds every project's turns, so each word fetched about 10,000 rows from the table to keep about 900. With
-- project_id in the same GIN index (btree_gin), the index intersects the project and the word before the table is read.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_operator_turns_project_search_tsv
    ON memory.operator_turns USING gin (project_id, search_tsv);
