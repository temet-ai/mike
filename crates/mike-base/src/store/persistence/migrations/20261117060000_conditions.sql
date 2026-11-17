-- Conditions (tether#40, slice 1): a session registers what it is waiting for and what mike
-- does when it holds, then ends its turn.
--
-- launch.conditions holds one row per registration. machine_uuid names the machine whose
-- worker evaluates and fires it, so every machine runs the same loop with no lease. kind says
-- what is waited on (a clock time, a session's turn or end, a fleet event, a read-only SQL
-- fact) and spec carries that kind's small shape, decoded whole in Rust. not_before is when
-- the condition starts to count (events older than it are ignored); deadline, when set, turns
-- an unmet condition into a missed one. action says what mike does: return to the registering
-- session (push, resume or start), start a new session, or mail one.
--
-- Every change is additive: a binary older than this migration keeps serving.

SET LOCAL lock_timeout = '10s';

CREATE TABLE IF NOT EXISTS launch.conditions (
    id                 BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    condition_uuid     UUID        NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    machine_uuid       UUID        NOT NULL,
    created_by_session UUID,
    project_id         BIGINT      REFERENCES projects.projects(id) ON DELETE CASCADE,
    thread_slug        TEXT,
    kind               TEXT        NOT NULL
        CHECK (kind IN ('time', 'session_turn_ended', 'session_ended', 'fleet_event', 'sql')),
    spec               JSONB       NOT NULL DEFAULT '{}'::jsonb,
    not_before         TIMESTAMPTZ NOT NULL DEFAULT now(),
    deadline           TIMESTAMPTZ,
    check_every_secs   INTEGER     NOT NULL DEFAULT 300 CHECK (check_every_secs >= 60),
    last_checked_at    TIMESTAMPTZ,
    action             TEXT        NOT NULL CHECK (action IN ('return', 'start', 'message')),
    target_session     UUID,
    folder             TEXT,
    brief              TEXT        NOT NULL CHECK (btrim(brief) <> ''),
    state              TEXT        NOT NULL DEFAULT 'armed'
        CHECK (state IN ('armed', 'fired', 'missed', 'failed', 'cancelled')),
    fired_at           TIMESTAMPTZ,
    outcome            JSONB,
    attempts           INTEGER     NOT NULL DEFAULT 0,
    next_attempt_at    TIMESTAMPTZ,
    last_error         TEXT
);

COMMENT ON TABLE launch.conditions IS 'One registered wait: a session says what it waits for (kind, spec) and what mike does when it holds (action, brief), then ends its turn. The worker of machine_uuid evaluates armed rows, fires the action once, and records state, fired_at and outcome. A deadline that passes first fires the action as missed.';
COMMENT ON COLUMN launch.conditions.spec IS 'JSONB because each kind has its own small shape (a session uuid, a fleet run and event kind, a read-only query) that is decoded whole in Rust by kind; nothing filters or joins on a field inside it.';
COMMENT ON COLUMN launch.conditions.not_before IS 'Events older than this do not count, and the row is not evaluated before it. A time condition holds once now() reaches it.';
COMMENT ON COLUMN launch.conditions.outcome IS 'How the action ran: via (push, mail, resume or start), the launch id and session when one was started, and whether the deadline passed first.';
COMMENT ON COLUMN launch.conditions.next_attempt_at IS 'Set after a failed firing attempt: the earliest time the worker tries again (1, 5, then 30 minutes); the row becomes failed after four attempts.';

CREATE INDEX IF NOT EXISTS idx_conditions_machine_armed
    ON launch.conditions (machine_uuid, not_before) WHERE state = 'armed';
CREATE INDEX IF NOT EXISTS idx_conditions_thread
    ON launch.conditions (thread_slug) WHERE thread_slug IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_conditions_created_by_session
    ON launch.conditions (created_by_session);
