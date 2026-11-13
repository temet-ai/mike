-- Issue #148 (Phase 24, A3): reconcile the workspace-registration /
-- capture project-identity split.
--
-- Before this migration, `POST /api/workspace/register` keyed
-- `projects.projects.project_path` on the caller's ABSOLUTE, worktree-
-- normalized cwd (`extract::request::compute_project_root`), while every
-- captured request keys it on the TRIMMED last-3-segments display form
-- (`extract::request::project_display_name`, applied by
-- `extract_project` -> `upsert_project_returning_id`). The same repo
-- therefore produced two rows. The application-code fix (this PR) makes
-- registration key on the trimmed form too; this migration reconciles the
-- rows that already exist under the old, absolute-path behavior.
--
-- Every absolute-path row (project_path LIKE '/%' — capture-produced rows
-- never start with '/', since project_display_name always strips the
-- leading empty segment) is canonicalized to the same trimmed form the
-- Rust helper computes: the last min(3, segments-2) path segments,
-- joined by '/'. Two outcomes per row:
--
--   (a) no pre-existing row at the canonical path -> renamed in place
--       (id unchanged, no FK repoint needed);
--   (b) a pre-existing row already holds the canonical path (the actual
--       duplicate) -> every FK referencing the absolute row is repointed
--       to the canonical row, first_seen/last_seen are merged, and the
--       absolute row is deleted.
--
-- When multiple absolute rows canonicalize to the same path and NEITHER
-- has a pre-existing canonical counterpart, the lowest id becomes the
-- target and the rest merge into it (mirrors the "two repos share a
-- basename" accepted residual already present in capture's own
-- resolution — this migration does not introduce new ambiguity, only
-- collapses to the same single-row behavior capture already exhibits).
--
-- Set-based per FK table; the row-by-row loop is over `projects.projects`
-- only (a handful of rows at most, not a request/session-scale table), so
-- it is a one-time reconciliation pass, not the per-request correlated
-- subquery the sql-set-based rule bans.
--
-- Every FK to projects.projects(id) as of this migration (verified via
-- pg_constraint against the live capture DB, not just a migration-file
-- grep): sessions.sessions, requests.requests, memory.operator_turns,
-- memory.lessons, workspace.session_registrations,
-- workspace.coordination_events.

SET LOCAL lock_timeout = '60s';

CREATE TEMP TABLE _project_identity_map ON COMMIT DROP AS
WITH absolute_rows AS (
    SELECT
        id,
        project_path,
        array_remove(string_to_array(project_path, '/'), '') AS segments
    FROM projects.projects
    WHERE project_path LIKE '/%'
),
canonicalized AS (
    SELECT
        id,
        project_path AS old_path,
        array_to_string(
            segments[
                (CASE
                     WHEN array_length(segments, 1) > 3 THEN array_length(segments, 1) - 3
                     WHEN array_length(segments, 1) > 2 THEN 2
                     ELSE 0
                 END) + 1
                : array_length(segments, 1)
            ],
            '/'
        ) AS canonical_path
    FROM absolute_rows
    WHERE array_length(segments, 1) > 0
),
existing_targets AS (
    SELECT c.canonical_path, MIN(p.id) AS existing_id
    FROM canonicalized c
    JOIN projects.projects p
      ON p.project_path = c.canonical_path
     AND p.id <> c.id
    GROUP BY c.canonical_path
),
fallback_targets AS (
    SELECT canonical_path, MIN(id) AS fallback_id
    FROM canonicalized
    GROUP BY canonical_path
)
SELECT
    c.id AS old_id,
    c.old_path,
    c.canonical_path,
    COALESCE(e.existing_id, f.fallback_id) AS target_id
FROM canonicalized c
LEFT JOIN existing_targets e ON e.canonical_path = c.canonical_path
JOIN fallback_targets f ON f.canonical_path = c.canonical_path;

-- Case (b): merge every duplicate into its target row.
DO $merge$
DECLARE
    r RECORD;
BEGIN
    FOR r IN SELECT old_id, target_id FROM _project_identity_map WHERE old_id <> target_id LOOP
        UPDATE sessions.sessions SET project_id = r.target_id WHERE project_id = r.old_id;
        UPDATE requests.requests SET project_id = r.target_id WHERE project_id = r.old_id;
        UPDATE memory.operator_turns SET project_id = r.target_id WHERE project_id = r.old_id;
        UPDATE memory.lessons SET project_id = r.target_id WHERE project_id = r.old_id;
        UPDATE workspace.session_registrations SET project_id = r.target_id WHERE project_id = r.old_id;
        UPDATE workspace.coordination_events SET project_id = r.target_id WHERE project_id = r.old_id;

        UPDATE projects.projects target
           SET first_seen = LEAST(target.first_seen, old.first_seen),
               last_seen  = GREATEST(target.last_seen, old.last_seen)
          FROM projects.projects old
         WHERE target.id = r.target_id AND old.id = r.old_id;

        DELETE FROM projects.projects WHERE id = r.old_id;
    END LOOP;
END;
$merge$;

-- Case (a): the surviving target row, when it was itself an absolute-path
-- row (not a pre-existing capture row), is renamed to its canonical form.
UPDATE projects.projects p
   SET project_path  = m.canonical_path,
       display_name  = m.canonical_path
  FROM (SELECT DISTINCT canonical_path, target_id FROM _project_identity_map) m
 WHERE p.id = m.target_id
   AND p.project_path <> m.canonical_path;
