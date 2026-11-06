-- Process-liveness reaping for `workspace.session_registrations` (staleness
-- reaper thread): a registration's lease only renews on the Stop hook, so a
-- session sitting idle mid-turn reads `stale` while alive, and a terminal
-- window closed without SessionEnd firing reads `stale` forever. `pid` +
-- `boot_id` let the sweeper ask the OS whether the registering process still
-- exists instead of trusting the lease clock.
--
-- Both columns are nullable: existing rows have neither, and MCP/REST
-- callers may register without a pid — an absent pid must behave exactly as
-- today (fail-open, never reaped). `boot_id` is server-derived at register
-- time (never client-supplied) so a stale value can't be spoofed; it also
-- guards against pid recycling — a row's boot_id must match the sweeper's
-- own current boot before its pid is trusted.
--
-- `reaped_at` keeps a sweeper-driven close distinguishable from an explicit
-- `end` call: both stamp `released_at`, but only the sweeper stamps
-- `reaped_at`. Overloading `released_at` alone would erase that distinction.

ALTER TABLE workspace.session_registrations
    ADD COLUMN pid       integer,
    ADD COLUMN boot_id   text,
    ADD COLUMN reaped_at timestamptz;

COMMENT ON COLUMN workspace.session_registrations.pid IS
    'OS process id of the registering session, when supplied. NULL rows are never reapable — pids recycle, so no pid means no liveness check.';
COMMENT ON COLUMN workspace.session_registrations.boot_id IS
    'Server-derived boot identifier (kern.boottime / /proc/stat btime) captured at register time — never client-supplied. A row whose boot_id predates the sweeper''s current boot is never reapable, since its pid may have been recycled since.';
COMMENT ON COLUMN workspace.session_registrations.reaped_at IS
    'Set alongside released_at only when the sweeper closed this row because its process was gone. NULL for a clean explicit end (register/end route) — the distinguishing signal between REAPED and ended.';

-- Sweeper lookup: unreleased rows with a pid, grouped by boot so a single
-- sweep tick can filter to its own current boot in one indexed scan.
CREATE INDEX idx_session_registrations_reapable
    ON workspace.session_registrations (boot_id)
    WHERE released_at IS NULL AND pid IS NOT NULL;

-- New ledger vocabulary entry for the sweeper's ledger event.
INSERT INTO workspace.event_kinds (id, code) OVERRIDING SYSTEM VALUE VALUES
    (5, 'reaped')
ON CONFLICT (id) DO NOTHING;
SELECT setval('workspace.event_kinds_id_seq', 5);
