-- Blocks mike added to a user message mid-thread (session identity, code findings, memory
-- keeper cards), kept so a full re-send after a proxy restart carries the same bytes the
-- continuation requests did and reads its history from the prompt cache.
--
-- One row per recorded block tail: machine_uuid is the machine whose proxy recorded it (each
-- machine loads only its own rows at startup), lane names the session's request lane,
-- message_identity is the stable hash of the user message the tail follows, and tail is the
-- exact bytes spliced after that message's content. Rows older than the one-hour prompt-cache
-- window are useless and are deleted by the proxy. A later row for the same lane and message
-- wins, so there is no uniqueness constraint.
--
-- Additive: a binary older than this migration never reads or writes the table.

SET LOCAL lock_timeout = '10s';

CREATE TABLE IF NOT EXISTS overrides.injected_replay_records (
    id               BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    machine_uuid     UUID        NOT NULL,
    lane             TEXT        NOT NULL,
    message_identity BIGINT      NOT NULL,
    tail             BYTEA       NOT NULL,
    recorded_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_injected_replay_records_machine_recorded
    ON overrides.injected_replay_records (machine_uuid, recorded_at);
