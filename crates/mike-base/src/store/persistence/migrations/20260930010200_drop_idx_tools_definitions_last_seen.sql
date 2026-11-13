-- no-transaction
-- 1.3 MB, no reader that needs last_seen order; frees last_seen updates to be HOT.
DROP INDEX CONCURRENTLY IF EXISTS tools.idx_tools_definitions_last_seen;
