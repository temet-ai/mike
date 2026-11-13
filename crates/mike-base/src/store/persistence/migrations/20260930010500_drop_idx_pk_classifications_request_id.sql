-- no-transaction
-- Duplicate of uq_pk_classifications_request_id, 0 scans.
DROP INDEX CONCURRENTLY IF EXISTS prompt_kinds.idx_pk_classifications_request_id;
