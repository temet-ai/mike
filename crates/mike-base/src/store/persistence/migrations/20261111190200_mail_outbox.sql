-- Durable outbox for mail delivered through a runtime adapter (Claude mailbox, Codex app-server).
-- message_uuid is the logical message identity and is kept across retries; each delivery try is
-- its own attempt row with its own attempt_uuid, so every try stays diagnosable on its own.
-- generation is the recipient's ownership generation (its current launch) when the row was last
-- routed; evidence from an attempt of an older generation cannot advance the message.

CREATE TABLE mailbox.outbox (
    message_uuid uuid PRIMARY KEY,
    to_session uuid NOT NULL,
    from_session uuid NULL,
    runtime text NOT NULL CHECK (runtime IN ('claude', 'codex')),
    scope text NULL,
    body text NOT NULL,
    generation bigint NOT NULL,
    status text NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'delivered', 'unresolved', 'failed')),
    status_reason text NULL,
    queued_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

-- Recovery scan: the mail not yet settled, oldest first.
CREATE INDEX idx_mailbox_outbox_open ON mailbox.outbox (queued_at)
    WHERE status IN ('pending', 'unresolved');

CREATE TABLE mailbox.outbox_attempts (
    attempt_uuid uuid PRIMARY KEY,
    message_uuid uuid NOT NULL REFERENCES mailbox.outbox(message_uuid) ON DELETE CASCADE,
    attempt_no integer NOT NULL CHECK (attempt_no >= 1),
    generation bigint NOT NULL,
    mode text NOT NULL CHECK (mode IN ('start', 'steer', 'mailbox')),
    expected_turn_id text NULL,
    started_at timestamptz NOT NULL DEFAULT now(),
    finished_at timestamptz NULL,
    outcome text NULL CHECK (outcome IN (
        'accepted', 'turn_ended', 'refused', 'failed_before_send', 'failed_after_send'
    )),
    turn_id text NULL,
    detail text NULL,
    UNIQUE (message_uuid, attempt_no),
    CHECK ((mode = 'steer') = (expected_turn_id IS NOT NULL))
);

-- One row per stage that has evidence. A stage with no row is unknown, never assumed.
CREATE TABLE mailbox.outbox_stages (
    message_uuid uuid NOT NULL REFERENCES mailbox.outbox(message_uuid) ON DELETE CASCADE,
    stage text NOT NULL CHECK (stage IN (
        'queued', 'accepted', 'consumed', 'action_started', 'completed'
    )),
    attempt_uuid uuid NULL REFERENCES mailbox.outbox_attempts(attempt_uuid) ON DELETE SET NULL,
    evidence text NOT NULL CHECK (btrim(evidence) <> ''),
    observed_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (message_uuid, stage)
);

CREATE INDEX idx_mailbox_outbox_stages_attempt ON mailbox.outbox_stages (attempt_uuid)
    WHERE attempt_uuid IS NOT NULL;
