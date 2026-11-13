-- no-transaction
-- tell_inactive_senders (mailbox reaper, every 300 s) reads the oldest undelivered messages whose sender has not been told; the predicate matches undelivered_due_inactive.sql exactly.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_mailbox_messages_sender_unnotified
    ON mailbox.messages (queued_at)
 WHERE sender_notified_at IS NULL
   AND from_session_id IS NOT NULL
   AND to_session_id IS NOT NULL
   AND sent_at IS NULL
   AND sent_request_id IS NULL
   AND channel_delivered_at IS NULL
   AND consumed_at IS NULL
   AND dead_lettered_at IS NULL;
