-- Redelivery backoff gate for mailbox.messages (issue #181): a recipient
-- cycling through several eligible primary-lane turns inside a minute could
-- exhaust `mailbox_redelivery_max_attempts` and dead-letter a message while
-- the recipient was simply mid-turn, not gone. `next_retry_at` is the
-- earliest instant `bump_redelivery_attempt` may proceed again — NULL until
-- the first redelivery bump (so the very first redelivery after the initial
-- send stays immediate), then set to an exponentially growing, capped delay
-- after every subsequent bump.

ALTER TABLE mailbox.messages
    ADD COLUMN next_retry_at timestamptz;

COMMENT ON COLUMN mailbox.messages.next_retry_at IS
    'Redelivery backoff gate (issue #181): NULL until the first redelivery bump; thereafter the earliest instant bump_redelivery_attempt may proceed again, so repeated eligible turns in a short window cannot exhaust delivery_attempts inside a minute.';

-- Sender-facing dead-lettered-outbox count (workspace heartbeat, issue
-- #181): small, highly selective partial index over dead-lettered rows
-- keyed by sender — dead-lettered rows are the rare case, so the index
-- stays tiny even as mailbox.messages grows.
CREATE INDEX idx_mailbox_messages_from_session_dead_lettered
    ON mailbox.messages (from_session_id)
    WHERE dead_lettered_at IS NOT NULL;
