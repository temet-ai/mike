-- Reverses the merge direction of
-- 20260712000003_workspace_project_identity_unification.sql. That migration
-- collapsed every full-path project row onto the trimmed last-3-segments
-- form to match request capture's (then) behavior. Commit 8d09f95f and its
-- companion 20260722000000_project_identity_from_session_cwd.sql reversed
-- the policy — project identity is the full canonical Git repository root,
-- never trimmed (repeat operator correction, 2026-07-22) — but nothing
-- folded the surviving trimmed rows forward onto the new full-path rows a
-- confirmed Git resolution creates. A project whose path is the trimmed
-- form of another project's full path is the same repository under two
-- ids, including every worktree checkout of that repository (worktrees
-- share the main checkout's `--git-common-dir`, so they canonicalize to the
-- same full root).
--
-- Every FK to projects.projects(id), same set 20260712000003 verified
-- against the live capture DB: sessions.sessions, requests.requests,
-- memory.operator_turns, memory.lessons, workspace.session_registrations,
-- workspace.coordination_events, steering.steerers.
--
-- Set-based per FK table; the row-by-row loop is over projects.projects
-- only (a handful of rows at most, not a request/session-scale table), so
-- it is a one-time reconciliation pass, not the per-request correlated
-- subquery the sql-set-based rule bans.

SET LOCAL lock_timeout = '60s';

CREATE TEMP TABLE _absolute_projects ON COMMIT DROP AS
WITH segmented AS (
    SELECT id, project_path,
           array_remove(string_to_array(project_path, '/'), '') AS segments
    FROM projects.projects
    WHERE project_path LIKE '/%'
)
SELECT
    id,
    project_path,
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
    ) AS trimmed_form
FROM segmented
WHERE array_length(segments, 1) > 0;

-- DISTINCT ON keeps this deterministic even if more than one absolute row
-- happened to trim to the same form (lowest target id wins, same
-- tie-break 20260712000003 used).
CREATE TEMP TABLE _project_merge_map ON COMMIT DROP AS
SELECT DISTINCT ON (trimmed.id)
    trimmed.id             AS old_id,
    trimmed.project_path   AS old_path,
    abs_match.id           AS target_id,
    abs_match.project_path AS target_path
FROM projects.projects trimmed
JOIN _absolute_projects abs_match ON abs_match.trimmed_form = trimmed.project_path
WHERE trimmed.project_path NOT LIKE '/%'
ORDER BY trimmed.id, abs_match.id;

DO $merge$
DECLARE
    r RECORD;
BEGIN
    FOR r IN SELECT old_id, target_id FROM _project_merge_map LOOP
        UPDATE sessions.sessions SET project_id = r.target_id WHERE project_id = r.old_id;
        UPDATE requests.requests SET project_id = r.target_id WHERE project_id = r.old_id;
        UPDATE memory.operator_turns SET project_id = r.target_id WHERE project_id = r.old_id;
        UPDATE memory.lessons SET project_id = r.target_id WHERE project_id = r.old_id;
        UPDATE workspace.session_registrations SET project_id = r.target_id WHERE project_id = r.old_id;
        UPDATE workspace.coordination_events SET project_id = r.target_id WHERE project_id = r.old_id;
        UPDATE steering.steerers SET project_id = r.target_id WHERE project_id = r.old_id;

        UPDATE projects.projects target
           SET first_seen     = LEAST(target.first_seen, old.first_seen),
               last_seen       = GREATEST(target.last_seen, old.last_seen),
               pinned          = target.pinned OR old.pinned,
               pin_created_at  = COALESCE(target.pin_created_at, old.pin_created_at)
          FROM projects.projects old
         WHERE target.id = r.target_id AND old.id = r.old_id;

        DELETE FROM projects.projects WHERE id = r.old_id;
    END LOOP;
END;
$merge$;

-- Every session now grouped under its repository's full canonical root
-- (main checkout or any worktree) has a proven-correct project link. Heal
-- the sentinel 'unresolved' provenance the same way a live Git resolution
-- would — an explicit 'operator' assignment is never matched by this
-- filter, so it stays untouched.
UPDATE sessions.sessions s
   SET project_resolution = 'git'
  FROM projects.projects p
 WHERE s.project_id = p.id
   AND s.project_resolution = 'unresolved'
   AND p.project_path LIKE '/%'
   AND s.cwd LIKE p.project_path || '%';
