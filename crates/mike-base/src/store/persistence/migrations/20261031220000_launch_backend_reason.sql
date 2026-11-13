-- Why the launcher chose a launch's terminal backend (#691): the caller's own
-- terminal (a Herdr app terminal, an old herdr pane, tmux), a backend found
-- running, an explicit request, or the configured launch_backend. NULL for
-- launches recorded before this column existed.

ALTER TABLE launch.launches ADD COLUMN IF NOT EXISTS backend_reason text;

-- The herdr-core backend (the Herdr desktop app) joins the closed list set by
-- 20260916000001_launch_domain.sql.
ALTER TABLE launch.launches DROP CONSTRAINT IF EXISTS launches_backend_check;
ALTER TABLE launch.launches
    ADD CONSTRAINT launches_backend_check
        CHECK (backend IN ('herdr', 'herdr-core', 'tmux', 'terminal', 'ghostty', 'none'));

COMMENT ON COLUMN launch.launches.backend_reason IS
    'Why the launcher chose this backend, in words: the caller''s terminal, a running backend, an explicit request, or the configured launch_backend. NULL before #691.';
