-- no-transaction
-- A session's inbox (fetch_inbox_with_activity, polled every 5 s per session) filters
-- mailbox.messages by to_session_id and orders by id; only partial indexes covered
-- to_session_id, so every poll scanned the whole table.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_mailbox_messages_to_session
    ON mailbox.messages (to_session_id, id);
