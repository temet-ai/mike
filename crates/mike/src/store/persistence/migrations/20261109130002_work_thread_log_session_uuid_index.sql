-- no-transaction
-- memory.seat_memory joins work.thread_log to a seat by session_uuid; the table had no index on it, so each read scanned and decompressed every entry body.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_work_thread_log_session_uuid ON work.thread_log (session_uuid) WHERE session_uuid IS NOT NULL;
