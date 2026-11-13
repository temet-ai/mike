-- How long a fleet worker forked from a template waited before it started,
-- because a sibling fork of the same template went first so this one could
-- start from the prompt cache the first one wrote. NULL when the seat did not
-- wait. Only adds nullable columns; a server on an older build ignores them.

ALTER TABLE launch.launches
    ADD COLUMN IF NOT EXISTS sibling_wait_seconds integer NULL
        CHECK (sibling_wait_seconds IS NULL OR sibling_wait_seconds >= 0),
    ADD COLUMN IF NOT EXISTS waited_for_seat text NULL;
