-- no-transaction
-- idx_request_deltas_session_request leads with session_id, so it serves every lookup the
-- session_id-only index did (#353).
DROP INDEX CONCURRENTLY IF EXISTS requests.idx_request_deltas_session_id;
