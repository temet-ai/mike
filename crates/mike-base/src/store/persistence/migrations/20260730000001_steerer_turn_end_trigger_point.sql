-- Add 'turn_end' as a valid steering.steerers.trigger_point value and flip
-- the false-done registry row onto it: the fleet only judges responses whose
-- stop_reason is 'end_turn' (with non-empty text) for turn_end steerers.

ALTER TABLE steering.steerers
    DROP CONSTRAINT IF EXISTS steerers_trigger_point_check;

ALTER TABLE steering.steerers
    ADD CONSTRAINT steerers_trigger_point_check
        CHECK (trigger_point IN ('user_prompt', 'response', 'turn_end'));

UPDATE steering.steerers
   SET trigger_point = 'turn_end'
 WHERE name = 'false-done' AND kind = 'false_done';
