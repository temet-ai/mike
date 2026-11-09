-- What memory-keeper supplied to each session: one row per (session, thread,
-- card text), written after the block was returned so the route's latency does
-- not include it. A repeat of the same card for the same session is not
-- recorded again; the writer checks with INSERT ... WHERE NOT EXISTS over
-- (session_uuid, thread_id, content_hash), served by the (session_uuid,
-- thread_id) index. There is no UNIQUE or index on content_hash: it is a SHA.
--
-- A control-arm session gets no card and is recorded with arm 'control' and no
-- thread, so the rate view can compare the two arms. resolution says whether the
-- resolver found one thread, several close ones, or none.

CREATE TABLE IF NOT EXISTS memory.keeper_supplies (
    id            bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    session_uuid  uuid        NOT NULL,
    thread_id     bigint      NULL REFERENCES work.threads(id) ON DELETE CASCADE,
    content_hash  text        NULL,
    carrier       text        NOT NULL,
    arm           text        NOT NULL,
    resolution    text        NOT NULL,
    signals       jsonb       NOT NULL DEFAULT '{}'::jsonb,
    created_at    timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT keeper_supplies_carrier_check CHECK (carrier IN ('tether', 'mod', 'proxy')),
    CONSTRAINT keeper_supplies_arm_check CHECK (arm IN ('keeper', 'control')),
    CONSTRAINT keeper_supplies_resolution_check CHECK (resolution IN ('one', 'several', 'none'))
);

CREATE INDEX IF NOT EXISTS idx_keeper_supplies_session_thread
    ON memory.keeper_supplies (session_uuid, thread_id);

COMMENT ON TABLE memory.keeper_supplies IS
    'One row per card memory-keeper supplied to a session (or per control-arm or unresolved prompt), recorded off the response path. The source of the supply rate and of resolution accuracy.';
COMMENT ON COLUMN memory.keeper_supplies.session_uuid IS
    'The session the card was supplied to. A soft reference: the session row may arrive after the supply.';
COMMENT ON COLUMN memory.keeper_supplies.thread_id IS
    'The thread whose card was supplied. NULL for a control-arm session and for a prompt that resolved to no thread.';
COMMENT ON COLUMN memory.keeper_supplies.content_hash IS
    'work.thread_cards.content_hash of the card supplied. Not indexed.';
COMMENT ON COLUMN memory.keeper_supplies.carrier IS
    'tether: the /context block. mod: the for-prompt field. proxy: the proxy stage.';
COMMENT ON COLUMN memory.keeper_supplies.arm IS
    'keeper: the session receives cards. control: the holdout, recorded without a card.';
COMMENT ON COLUMN memory.keeper_supplies.resolution IS
    'one: a single thread was supplied. several: a short list of close threads. none: nothing was supplied.';
COMMENT ON COLUMN memory.keeper_supplies.signals IS
    'The resolver signals that decided the resolution, for accuracy checks.';
