-- launch.fleets: a fleet definition, stored before any seat exists.
--
-- A fleet run today is stood up by hand from a coordinator's notes. This
-- table holds the definition an operator (or a session on their behalf)
-- builds up over time: which seats, which agents and models, which
-- criteria. `mike fleet start` reads the definition, the check result says
-- whether it is complete, and the start records which seats it launched.
--
-- One row per fleet. A row moves draft -> ready -> starting -> running ->
-- done | failed. The definition and the last check result are JSONB because
-- they are documents the fleet code reads whole and validates in Rust;
-- nothing filters, sorts or joins on a field inside them. Everything a list
-- or a status page does select on (slug, project_path, state, run_id, the
-- stamps) is a typed column.
--
-- Additive and re-runnable: IF NOT EXISTS on every object.

CREATE TABLE IF NOT EXISTS launch.fleets (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    fleet_uuid uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    slug text NOT NULL,
    project_path text NOT NULL,
    state text NOT NULL DEFAULT 'draft'
        CONSTRAINT fleets_state_check
        CHECK (state IN ('draft', 'ready', 'starting', 'running', 'failed', 'done')),
    definition jsonb NOT NULL DEFAULT '{}'::jsonb,
    missing jsonb NOT NULL DEFAULT '[]'::jsonb,
    -- A run going away must not erase the fleet that started it.
    run_id text REFERENCES launch.runs(run_id) ON DELETE SET NULL,
    start_report jsonb,
    created_by uuid,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    ready_at timestamptz,
    started_at timestamptz,
    finished_at timestamptz
);

COMMENT ON TABLE launch.fleets IS
    'A fleet definition and its progress: the seats, agents, models and '
    'criteria an operator has described, whether the checks pass, and which '
    'run the start launched. One row per fleet; state moves draft, ready, '
    'starting, running, then done or failed.';
COMMENT ON COLUMN launch.fleets.fleet_uuid IS
    'The external handle every route and command resolves by. The bigint id '
    'never leaves the store layer.';
COMMENT ON COLUMN launch.fleets.slug IS
    'A short name the operator types. Not unique: a slug resolves to the '
    'newest fleet carrying it.';
COMMENT ON COLUMN launch.fleets.state IS
    'draft: still being described. ready: every check passes. starting: the '
    'start is launching seats. running: seats are open. failed and done are '
    'terminal.';
COMMENT ON COLUMN launch.fleets.definition IS
    'The fleet definition document (seats, agents, models, criteria). JSONB '
    'because it is a nested document the fleet code validates whole in Rust '
    'and nothing filters or joins on a field inside it.';
COMMENT ON COLUMN launch.fleets.missing IS
    'The last check result: an array of {item, problem}. Empty means every '
    'check passed. Replaced whole on each save, never queried by field.';
COMMENT ON COLUMN launch.fleets.run_id IS
    'The launch run the start opened its seats in. NULL until the start '
    'names one. ON DELETE SET NULL keeps the fleet when its run is removed.';
COMMENT ON COLUMN launch.fleets.start_report IS
    'One entry per seat the start launched, as reported by the launcher. '
    'NULL until a start has run. Read whole, never filtered by field.';
COMMENT ON COLUMN launch.fleets.created_by IS
    'The session that created the fleet. Not a foreign key: the creating '
    'session may not have a sessions.sessions row yet when it defines a '
    'fleet, and losing the fleet with the session would be wrong.';
COMMENT ON COLUMN launch.fleets.updated_at IS
    'Bumped on every write. A definition save compares it with the value the '
    'writer read, so two writers cannot silently overwrite each other.';
COMMENT ON COLUMN launch.fleets.ready_at IS
    'When the fleet last became ready. NULL until it has.';
COMMENT ON COLUMN launch.fleets.started_at IS
    'When the fleet last reached running. NULL until it has.';
COMMENT ON COLUMN launch.fleets.finished_at IS
    'When the fleet last reached failed or done. NULL until it has.';

CREATE INDEX IF NOT EXISTS idx_launch_fleets_project_created
    ON launch.fleets (project_path, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_launch_fleets_slug_created
    ON launch.fleets (slug, created_at DESC);

-- The run FK's own lookup, and the ON DELETE SET NULL walk.
CREATE INDEX IF NOT EXISTS idx_launch_fleets_run_id
    ON launch.fleets (run_id)
    WHERE run_id IS NOT NULL;
