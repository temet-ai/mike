-- Conditions slice 2 (tether#40): a `words` condition is a plain-words statement about a
-- watched session, judged by the side-call chain only after that session ends a new turn.
--
-- judged_through is the time the last answered judgement read the watched session's turns
-- from; the next judgement waits for a turn that closed after it. asked_at is set the first
-- time the judge was unsure and mike mailed the watched session to ask, so it asks once.
--
-- Every change is additive: a binary older than this migration keeps serving.

SET LOCAL lock_timeout = '10s';

ALTER TABLE launch.conditions DROP CONSTRAINT IF EXISTS conditions_kind_check;
ALTER TABLE launch.conditions ADD CONSTRAINT conditions_kind_check
    CHECK (kind IN ('time', 'session_turn_ended', 'session_ended', 'fleet_event', 'sql', 'words'));

ALTER TABLE launch.conditions ADD COLUMN IF NOT EXISTS judged_through TIMESTAMPTZ;
ALTER TABLE launch.conditions ADD COLUMN IF NOT EXISTS asked_at TIMESTAMPTZ;

COMMENT ON COLUMN launch.conditions.judged_through IS 'words kind: the watched session''s turns that closed before this were already judged; the next judgement waits for a later turn.';
COMMENT ON COLUMN launch.conditions.asked_at IS 'words kind: when mike mailed the watched session because the judge could not tell from its turns; set once, so mike asks once.';
