-- The configured acceptance session of a fleet, promoted from its definition so the
-- per-session fleet lookup filters on a short inline column instead of detoasting every
-- definition in the project.
ALTER TABLE launch.fleets
    ADD COLUMN IF NOT EXISTS acceptance_session text
        GENERATED ALWAYS AS (definition #>> '{acceptance,session}') STORED;

COMMENT ON COLUMN launch.fleets.acceptance_session IS
    'definition.acceptance.session as written; matched case- and space-insensitively by find_fleet_for_acceptance.sql';
