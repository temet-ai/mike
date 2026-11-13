-- memory-keeper also supplies earlier work: when a prompt points at the past
-- ("we fixed this last week", a ticket key), the keeper searches past sessions
-- and supplies the top sessions with a reference each. Those supplies are
-- recorded in memory.keeper_supplies with resolution 'earlier', no thread, the
-- block's hash as content_hash, and the query and references in signals.

ALTER TABLE memory.keeper_supplies
    DROP CONSTRAINT IF EXISTS keeper_supplies_resolution_check;

ALTER TABLE memory.keeper_supplies
    ADD CONSTRAINT keeper_supplies_resolution_check
    CHECK (resolution IN ('one', 'several', 'none', 'earlier'));

COMMENT ON COLUMN memory.keeper_supplies.resolution IS
    'one: a single thread was supplied. several: a short list of close threads. none: nothing was supplied. earlier: past sessions a past-pointing prompt may refer to (signals holds the query and references).';
