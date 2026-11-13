ALTER TABLE launch.fleets
    DROP CONSTRAINT fleets_state_check;

ALTER TABLE launch.fleets
    ADD CONSTRAINT fleets_state_check
    CHECK (state IN ('draft', 'ready', 'starting', 'running', 'stopping', 'failed', 'done'));

COMMENT ON COLUMN launch.fleets.state IS
    'draft: still being described. ready: every check passes. starting: the '
    'start is launching seats. running: seats are open. stopping: new work is '
    'fenced while launch exits and managed command receipts settle. failed '
    'and done are terminal.';
