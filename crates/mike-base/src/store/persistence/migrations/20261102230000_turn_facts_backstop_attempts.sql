-- The dossier backstop re-served every turn whose captured sources were not yet
-- consistent on each 60 s tick, forever, and such sessions sorted first and filled
-- the batch. Each time the backstop serves a stale turn it now records an attempt:
-- the turn is left alone until retry_after (60 s doubling, capped at 1 h) and for
-- good once unbuildable_at is set. Writing the turn's facts clears its row.
CREATE TABLE memory.turn_facts_backstop_attempts (
    operator_turn_id bigint PRIMARY KEY
        REFERENCES memory.operator_turns (id) ON DELETE CASCADE,
    attempt_count integer NOT NULL CHECK (attempt_count > 0),
    last_attempt_at timestamptz NOT NULL,
    retry_after timestamptz NOT NULL,
    unbuildable_at timestamptz
);

COMMENT ON TABLE memory.turn_facts_backstop_attempts IS
    'Stale turns the dossier backstop has served without their facts being written; the candidates query skips a turn until retry_after, and permanently once unbuildable_at is set.';
