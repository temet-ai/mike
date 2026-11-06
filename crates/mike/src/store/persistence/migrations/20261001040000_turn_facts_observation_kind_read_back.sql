--
-- The steering-facts builder records observation_kind = 'read_back' when a session read back what
-- it had just written, and the constraint from 20260925000200 did not list it, so every dossier
-- batch containing such a turn failed. The list below is every kind the code can write
-- (ObservationKind::code() plus READ_BACK_KIND); a unit test compares it with the Rust list.
--
-- Idempotent: the constraint is dropped if present and re-added with the wider list; no row is
-- rewritten and existing values stay valid.

ALTER TABLE memory.turn_facts DROP CONSTRAINT IF EXISTS turn_facts_observation_kind_check;
ALTER TABLE memory.turn_facts
    ADD CONSTRAINT turn_facts_observation_kind_check
        CHECK (observation_kind IS NULL
               OR observation_kind IN ('test', 'build', 'lint', 'gate', 'run', 'http', 'browser',
                                       'render', 'deploy_poll', 'read_back'));
