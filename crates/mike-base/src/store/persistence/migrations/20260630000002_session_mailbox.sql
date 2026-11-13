CREATE SCHEMA IF NOT EXISTS mailbox;

CREATE TABLE mailbox.messages (
    id                bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    message_uuid      uuid NOT NULL UNIQUE,
    to_session_id     bigint NOT NULL REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    from_session_id   bigint REFERENCES sessions.sessions(id) ON DELETE SET NULL,
    body              text   NOT NULL,
    queued_at         timestamptz NOT NULL DEFAULT now(),
    claimed_at        timestamptz,
    sent_request_id   bigint REFERENCES requests.requests(id) ON DELETE SET NULL,
    sent_at           timestamptz
);

COMMENT ON COLUMN mailbox.messages.from_session_id IS
    'Optional reply pointer to the sending session — a convenience for the recipient to reply, not an identity or auth claim. NULL when the sender is anonymous; SET NULL if that session is deleted.';
COMMENT ON COLUMN mailbox.messages.claimed_at IS
    'Two-phase delivery sentinel: set when a turn claims this row for injection, before the in-memory reminder is consumed. The pending-reload query excludes claimed rows so a restart between claim and stamp cannot re-deliver.';

-- Index every FK. The from_session and sent_request indexes are partial because both columns are
-- predominantly NULL (anonymous sends; rows pending delivery), so the index only carries live rows.
CREATE INDEX idx_mailbox_messages_from_session ON mailbox.messages (from_session_id) WHERE from_session_id IS NOT NULL;
CREATE INDEX idx_mailbox_messages_sent_request ON mailbox.messages (sent_request_id)  WHERE sent_request_id IS NOT NULL;
-- Reload/drain lookup: undelivered rows for one recipient, oldest first.
CREATE INDEX idx_mailbox_messages_pending ON mailbox.messages (to_session_id, id) WHERE sent_request_id IS NULL;
