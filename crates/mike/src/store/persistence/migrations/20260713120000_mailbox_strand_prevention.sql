-- Closes #185 (a never-delivered message could be acked, then re-surface as
-- fresh mail once its first-send eligibility query stopped honoring
-- consumed_at) and #188 (mail queued for a session that never turns again
-- had no expiry, so it stayed first-send-eligible forever). Widens the
-- failed_sends reason vocabulary for the two new failure branches this adds
-- (send-time liveness rejection, reap-time TTL expiry), adds the partial
-- index the TTL reaper's sweep rides, and backfills every row already stuck
-- in the #188 state.

ALTER TABLE mailbox.failed_sends
    DROP CONSTRAINT failed_sends_reason_check;

ALTER TABLE mailbox.failed_sends
    ADD CONSTRAINT failed_sends_reason_check
    CHECK (reason IN (
        'empty_message',
        'invalid_from_session_id',
        'session_not_found',
        'from_session_not_found',
        'database_error',
        'recipient_ended',
        'strand_ttl_expired'
    ));

-- Reaper sweep predicate: never-sent, unconsumed, not yet dead-lettered,
-- ordered by queued_at for the TTL cutoff range scan.
CREATE INDEX idx_mailbox_messages_reapable
    ON mailbox.messages (queued_at)
    WHERE sent_request_id IS NULL AND consumed_at IS NULL AND dead_lettered_at IS NULL;

-- One-time backfill: every row already stranded under the #188 bug (never
-- sent, never consumed, never dead-lettered) is dead-lettered now rather
-- than staying armed for indefinite re-injection. Stamps a timestamp only —
-- no row is deleted, and the reaper added in this same change keeps the
-- invariant closed going forward.
UPDATE mailbox.messages
   SET dead_lettered_at = now()
 WHERE sent_request_id IS NULL
   AND consumed_at IS NULL
   AND dead_lettered_at IS NULL;
