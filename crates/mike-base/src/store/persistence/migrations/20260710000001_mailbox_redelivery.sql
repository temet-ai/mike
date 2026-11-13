-- Durable redelivery + receipt states for mailbox.messages (issue #169 Unit A).
-- consumed_at: explicit ack via POST /api/sessions/{id}/messages/{uuid}/ack —
-- stops redelivery. dead_lettered_at: delivery_attempts reached the configured
-- cap without an ack. delivery_attempts: bumped once per successful envelope
-- injection (first send counts as attempt 1).

ALTER TABLE mailbox.messages
    ADD COLUMN consumed_at       timestamptz,
    ADD COLUMN dead_lettered_at  timestamptz,
    ADD COLUMN delivery_attempts int NOT NULL DEFAULT 0;

COMMENT ON COLUMN mailbox.messages.consumed_at IS
    'Stamped by POST /api/sessions/{id}/messages/{uuid}/ack. Once set the row is no longer redelivery-eligible.';
COMMENT ON COLUMN mailbox.messages.dead_lettered_at IS
    'Stamped when delivery_attempts reaches mailbox_redelivery_max_attempts without an ack — stops redelivery; surfaced via the mailbox staleness view instead of retried forever.';
COMMENT ON COLUMN mailbox.messages.delivery_attempts IS
    'Count of successful envelope injections (first send + every redelivery). Bumped by backfill_mailbox_sent_request_id (first send) and bump_redelivery_attempt (subsequent sends).';

-- Redelivery reload: already-sent rows still eligible for re-injection
-- (unconsumed, not dead-lettered). idx_mailbox_messages_pending (sent_request_id
-- IS NULL) covers only never-sent rows; this partial index backs the second,
-- additive reload query for the redelivery case.
CREATE INDEX idx_mailbox_messages_redelivery
    ON mailbox.messages (to_session_id, id)
    WHERE consumed_at IS NULL AND dead_lettered_at IS NULL AND sent_at IS NOT NULL;

-- Feature toggle: mailbox_redelivery (session-scoped, default ON — no-default-off rule).
ALTER TABLE config.feature_toggles
    DROP CONSTRAINT IF EXISTS feature_toggles_key_check;

ALTER TABLE config.feature_toggles
    ADD CONSTRAINT feature_toggles_key_check
    CHECK (key IN (
        'reminders',
        'recall_injector',
        'weasel_intercept',
        'steerer',
        'lesson_worker',
        'harvester',
        'db_resync',
        'judge_worker',
        'session_judge',
        'reminder_on_token',
        'mailbox_redelivery'
    ));

INSERT INTO config.feature_toggles (key, enabled) VALUES
    ('mailbox_redelivery', true)
ON CONFLICT (key) DO NOTHING;
