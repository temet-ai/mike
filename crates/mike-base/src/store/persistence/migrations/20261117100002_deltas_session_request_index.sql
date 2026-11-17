-- no-transaction
-- find_text's session scan filters requests.deltas by session_id and a window of request_ids
-- (#353). With single-column indexes the planner ANDed the session_id index with the
-- request_id index, reading every delta of the session to keep one window's rows.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_request_deltas_session_request ON requests.deltas (session_id, request_id) INCLUDE (change_kind_id, message_id, ord);
