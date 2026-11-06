-- no-transaction
-- 0 scans since statistics began, 399 MB; no query filters requests.deltas by removed_reason_id and the lookup table it references is never deleted from.
DROP INDEX CONCURRENTLY IF EXISTS requests.idx_request_deltas_removed_reason;
