-- no-transaction
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_runtime_outbox_fairness
    ON mailbox.outbox (updated_at, queued_at, message_uuid)
    WHERE status IN ('pending', 'unresolved');
