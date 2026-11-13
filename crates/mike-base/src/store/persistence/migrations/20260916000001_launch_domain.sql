-- The `launch` schema: mike's own terminal launcher (GitHub issue #257, the
-- first step of #256 — folding the `spawn` zsh CLI into mike as a launch
-- domain with an operator-chosen, setup-validated terminal backend).
--
-- spawn keeps its state in TSV ledgers and a JSONL event file on one
-- machine. Everything a launch knows is worth a row here instead: the
-- roster, the fleet coordinator and the inspection UI all
-- want to join "which surface is this session in" and "what was this
-- session told at launch" against the sessions and channels mike already
-- captures. Nothing on this host is migrated from the TSV files; there is
-- nothing in them worth carrying.
--
-- Four tables:
--   * `launches` — one row per launch attempt, whatever its outcome. The
--     full claude argv and the full brief text live on the row (inspection
--     fidelity: a truncated brief once hid a lost prompt for days, #244).
--   * `templates` — warm template sessions that `fork` starts from. Only the
--     table ships here; the warm/fork verbs arrive with #259.
--   * `runs` — a run id groups launches into one tab of the project's
--     workspace where the backend can place them (#34).
--   * `backend_validations` — every live validation of a terminal backend,
--     whoever triggered it (setup wizard, doctor, settings page, a launch).
--
-- Every statement is additive and re-runnable: IF NOT EXISTS on every
-- object, COMMENT ON is naturally idempotent, and the one ALTER that
-- touches an existing table drops and re-adds a constraint under a fixed
-- name, so a second run converges on the same catalog state.

CREATE SCHEMA IF NOT EXISTS launch;

COMMENT ON SCHEMA launch IS
    'mike''s terminal launcher: launch attempts, warm templates, runs and '
    'backend validations. Owned by crates/mike/src/launch and read through '
    'store/persistence/launch.';

-- Templates come first because launches reference them.
CREATE TABLE IF NOT EXISTS launch.templates (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name text NOT NULL,
    -- The template session's own id, preassigned by the launcher. A fork
    -- passes it as `--resume <session_uuid> --fork-session`.
    session_uuid uuid NOT NULL,
    folder text NOT NULL,
    backend text NOT NULL,
    model text,
    agent text,
    -- The complete claude argv the template was warmed with. A fork
    -- prepends its own resume flags to exactly this list, and refuses a
    -- prefix change without an explicit opt-in, so the list must be whole
    -- — JSONB because it is an ordered argv, not a set of facts anything
    -- filters on.
    claude_args jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    -- A re-warm under the same name closes the old row by pointing it at
    -- the new one rather than rewriting it: forks already taken from the
    -- old template keep a template_id that still resolves.
    superseded_by bigint REFERENCES launch.templates(id) ON DELETE SET NULL,
    closed_at timestamptz
);

COMMENT ON TABLE launch.templates IS
    'Warm template sessions that `mike spawn fork` starts from. A name is '
    'unique among live templates only; a re-warm supersedes the previous '
    'row instead of updating it, so historical forks keep a resolvable '
    'template_id.';
COMMENT ON COLUMN launch.templates.claude_args IS
    'The complete claude argv the template was warmed with, in order. A '
    'fork prepends --resume/--fork-session/--session-id to exactly this list.';
COMMENT ON COLUMN launch.templates.superseded_by IS
    'The template that replaced this one under the same name. NULL while '
    'this row is the live template for its name.';

-- One live template per name. The predicate matches the lookup exactly:
-- a fork resolves a name among rows that are neither superseded nor closed.
CREATE UNIQUE INDEX IF NOT EXISTS idx_launch_templates_live_name
    ON launch.templates (name)
    WHERE superseded_by IS NULL AND closed_at IS NULL;

-- The self-referencing FK's own lookup, and the ON DELETE SET NULL walk.
CREATE INDEX IF NOT EXISTS idx_launch_templates_superseded_by
    ON launch.templates (superseded_by)
    WHERE superseded_by IS NOT NULL;

CREATE TABLE IF NOT EXISTS launch.runs (
    run_id text PRIMARY KEY,
    project_path text,
    backend text NOT NULL,
    -- The backend's own handles for the workspace and tab this run owns.
    -- NULL on a backend without tab placement, which records the run and
    -- says so on stderr.
    workspace_ref text,
    tab_ref text,
    created_at timestamptz NOT NULL DEFAULT now(),
    closed_at timestamptz
);

COMMENT ON TABLE launch.runs IS
    'A run groups launches into one tab of the project''s workspace where '
    'the backend can place them. The first launch carrying a run id creates '
    'the tab and records it here; later launches split panes inside it.';
COMMENT ON COLUMN launch.runs.tab_ref IS
    'The backend''s opaque handle for the tab. NULL when the backend has no '
    'tab placement; the run still exists so list/kill --run can group by it.';

CREATE TABLE IF NOT EXISTS launch.launches (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,

    -- Nullable on purpose: a --resume or --continue launch without an
    -- explicit --session-id has no id the launcher can know before claude
    -- starts. Every other launch preassigns one.
    session_uuid uuid,

    backend text NOT NULL
        CHECK (backend IN ('herdr', 'tmux', 'terminal', 'ghostty', 'none')),
    folder text NOT NULL,
    repo_root text,
    worktree boolean NOT NULL DEFAULT false,

    -- Backend handles, opaque to mike: herdr workspace/pane ids, a tmux
    -- session name, a terminal's pid. Filled in when the backend reports
    -- the surface open, which is why they are nullable.
    workspace_ref text,
    tab_ref text,
    surface_ref text,
    pid integer,

    -- ON DELETE SET NULL, not CASCADE, for both: a run or template going
    -- away must not erase the record that a session was launched.
    run_id text REFERENCES launch.runs(run_id) ON DELETE SET NULL,
    seat text,
    template_id bigint REFERENCES launch.templates(id) ON DELETE SET NULL,

    model text,
    agent text,
    -- The full claude argv, in order, after mike's own flags. JSONB because
    -- it is an ordered list nothing filters on; model and agent are promoted
    -- beside it because those are what an operator lists by.
    claude_args jsonb NOT NULL DEFAULT '[]',

    prompt_present boolean NOT NULL DEFAULT false,
    -- How the brief reached claude: inline on the argv, or staged to a
    -- 0600 file above the inline size limit. NULL when there was no brief.
    brief_mode text CHECK (brief_mode IN ('inline', 'file')),
    brief_path text,
    -- The whole brief, never a slice. The brief is the one thing a launched
    -- session was told that the captured transcript can fail to show (#244
    -- was a brief that never arrived), so this row is the record of what
    -- was meant to arrive.
    brief_text text,

    -- Placement vocabulary is the backend's (tab/panel/window and split
    -- directions differ per backend), so these stay open text rather than
    -- carrying a CHECK that would have to know every backend.
    requested_placement text,
    actual_placement text,
    split_direction text,
    anchor_ref text,

    remote_control boolean NOT NULL DEFAULT true,
    proxy_env boolean NOT NULL DEFAULT true,

    -- The launcher's wait for a Remote Control channel on the new session.
    -- `found` is the only pass. NULL until the check has run, and stays
    -- NULL for a launch that never got as far as checking.
    channel_check text
        CHECK (channel_check IN ('found', 'timeout', 'db_unreachable', 'no_session_id', 'disabled')),
    channel_checked_at timestamptz,

    opened boolean NOT NULL DEFAULT false,
    -- Whether the brief was delivered to the surface. Three-valued on
    -- purpose: NULL means there was nothing to deliver, which is different
    -- from a delivery that failed.
    delivered boolean,
    confirmed boolean NOT NULL DEFAULT false,

    result text NOT NULL DEFAULT 'pending'
        CHECK (result IN ('pending', 'success', 'unconfirmed', 'failed')),
    reason text,

    -- The bounded capture of a failed backend call's stderr: at most 64 KiB
    -- with the flag set when more was cut. The only truncation this schema
    -- sanctions, because a backend's stderr is diagnostic noise of unbounded
    -- size, not inspection content.
    stderr_text text,
    stderr_truncated boolean NOT NULL DEFAULT false,

    created_at timestamptz NOT NULL DEFAULT now(),
    opened_at timestamptz,
    closed_at timestamptz,
    close_reason text,
    -- Whether the claude process had exited by the time the surface was
    -- closed. NULL when the backend could not tell.
    close_process_exited boolean
);

COMMENT ON TABLE launch.launches IS
    'One row per launch attempt by mike''s launcher, whatever the outcome. '
    'The row is written when the attempt starts, updated as the backend '
    'reports the surface open, when the outcome is known, and when the '
    'surface is closed. Never deleted: it is the record of what a session '
    'was told and where it ran.';
COMMENT ON COLUMN launch.launches.session_uuid IS
    'Preassigned by the launcher so the roster and the channel check can '
    'find the session before its first request. NULL for a --resume or '
    '--continue launch without an explicit --session-id.';
COMMENT ON COLUMN launch.launches.surface_ref IS
    'The backend''s opaque handle for the pane/window hosting claude '
    '(herdr pane id, tmux session name, terminal pid). Used by kill to '
    'find the surface again.';
COMMENT ON COLUMN launch.launches.claude_args IS
    'The complete claude argv after mike''s own flags, in order. Complete '
    'so a launch can be reproduced and a fork can compare prefixes.';
COMMENT ON COLUMN launch.launches.brief_text IS
    'The full brief text as handed to claude, with {{session_id}} already '
    'substituted. Never a slice.';
COMMENT ON COLUMN launch.launches.channel_check IS
    'Outcome of the wait for a Remote Control channel: found (pass), '
    'timeout, db_unreachable, no_session_id (nothing to wait for), '
    'disabled (opted out). NULL until the check has run.';
COMMENT ON COLUMN launch.launches.delivered IS
    'Whether the brief reached the surface. NULL when there was no brief to '
    'deliver, which is not the same as a failed delivery.';
COMMENT ON COLUMN launch.launches.stderr_text IS
    'Stderr of a failed backend call, capped at 64 KiB; stderr_truncated '
    'says whether more was cut.';
COMMENT ON COLUMN launch.launches.close_process_exited IS
    'Whether the claude process had exited when the surface was closed. '
    'NULL when the backend could not observe the process.';

-- kill --session and the roster join look a session up by its uuid.
CREATE INDEX IF NOT EXISTS idx_launch_launches_session_uuid
    ON launch.launches (session_uuid)
    WHERE session_uuid IS NOT NULL;

-- list --run / kill --run: a run's launches in creation order. Also the
-- run FK's own lookup.
CREATE INDEX IF NOT EXISTS idx_launch_launches_run_created
    ON launch.launches (run_id, created_at)
    WHERE run_id IS NOT NULL;

-- The template FK's lookup.
CREATE INDEX IF NOT EXISTS idx_launch_launches_template_id
    ON launch.launches (template_id)
    WHERE template_id IS NOT NULL;

-- Newest launches first for the Launches panel and `mike spawn list`.
CREATE INDEX IF NOT EXISTS idx_launch_launches_created_at
    ON launch.launches (created_at DESC);

-- Live launches per backend, for the surface cap (warn at 8, refuse at
-- 16). The predicate matches the count's own WHERE exactly.
CREATE INDEX IF NOT EXISTS idx_launch_launches_backend_open
    ON launch.launches (backend, closed_at)
    WHERE closed_at IS NULL;

CREATE TABLE IF NOT EXISTS launch.backend_validations (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    backend text NOT NULL,
    passed boolean NOT NULL,
    version text,
    protocol text,
    -- The validation report as it was shown: an array of {name, passed,
    -- detail, remedy}, one per sub-check, in the order they ran. JSONB
    -- rather than a child table because the report is displayed whole and
    -- never filtered by sub-check; `passed` beside it is the typed verdict
    -- everything actually queries.
    checks jsonb NOT NULL,
    started_at timestamptz NOT NULL,
    finished_at timestamptz NOT NULL,
    source text NOT NULL
        CHECK (source IN ('setup', 'doctor', 'settings', 'launch'))
);

COMMENT ON TABLE launch.backend_validations IS
    'Every live validation of a terminal backend, whoever triggered it. The '
    'newest row per backend is what setup and /settings show as the '
    'backend''s last known state; older rows are the history of when a '
    'backend stopped or started working.';
COMMENT ON COLUMN launch.backend_validations.checks IS
    'The sub-check report in run order: an array of {name, passed, detail, '
    'remedy}. Displayed whole; passed on the row is the queried verdict.';
COMMENT ON COLUMN launch.backend_validations.source IS
    'What triggered the validation: setup (the wizard), doctor (mike spawn '
    'doctor), settings (the settings page), launch (a probe before a launch).';

-- "The latest validation for this backend" is the only read.
CREATE INDEX IF NOT EXISTS idx_launch_backend_validations_backend_finished
    ON launch.backend_validations (backend, finished_at DESC);

-- The launcher registers the session on the roster the moment the backend
-- reports the surface open, under its own registered_via value so the
-- roster can tell a launcher-registered seat from one the hook registered.
-- The constraint was created inline and unnamed in
-- 20260707000001_workspace_core.sql, so Postgres named it; the DO block
-- drops whichever CHECK currently governs the column rather than assuming
-- that name, then the ADD re-creates it under a fixed name, which is what
-- makes a second run of this file converge.
DO $$
DECLARE
    existing text;
BEGIN
    FOR existing IN
        SELECT c.conname
          FROM pg_constraint c
         WHERE c.conrelid = 'workspace.session_registrations'::regclass
           AND c.contype = 'c'
           AND pg_get_constraintdef(c.oid) LIKE '%registered_via%'
    LOOP
        EXECUTE format(
            'ALTER TABLE workspace.session_registrations DROP CONSTRAINT %I',
            existing
        );
    END LOOP;
END
$$;

ALTER TABLE workspace.session_registrations
    ADD CONSTRAINT session_registrations_registered_via_check
    CHECK (registered_via IN ('hook', 'mcp', 'rest', 'passive', 'operator', 'launcher'));

COMMENT ON COLUMN workspace.session_registrations.registered_via IS
    'Who registered the session: hook (the SessionStart hook), mcp, rest, '
    'passive (back-filled from traffic), operator, or launcher (mike''s own '
    'launcher, which registers the seat as soon as the backend reports the '
    'surface open, before the session''s first request).';
