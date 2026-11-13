-- Heal sessions.sessions.project_id off the sentinel '(no project)' project for
-- rows the Rust-level heal in register_workspace_session (this thread) and
-- Gap A in insert_request_initial cannot reach retroactively — both only fire
-- on a live call, never on rows that were already stuck before either heal
-- shipped.
--
-- Two independent sources of truth, both already in the DB, no heuristics:
--
-- 1. workspace.session_registrations.project_id — the SessionStart hook's
--    real cwd-resolved project. A quota probe firing before the hook creates
--    the session row under the sentinel (resolve_session_id(None)); the hook
--    then registers the real project into session_registrations but, before
--    this thread's fix, never touched sessions.sessions.project_id. One row
--    per session_id (PRIMARY KEY), so this is a plain 1:1 heal.
--
-- 2. requests.requests.project_id — pre-2026-06-12 residue from before the
--    insert_request_initial heal (Gap A) shipped: a session sat on the
--    sentinel while its own captured requests already resolved a real
--    project. Where a session's requests disagree on project, the project
--    backing the most requests wins, tie broken on the lower project id
--    (DISTINCT ON, deterministic).
--
-- Both UPDATEs are guarded on sessions.sessions.project_id still being the
-- sentinel, so re-running this migration (or running it after either heal
-- has already fixed a row) touches 0 rows for anything already healed —
-- naturally idempotent, no IF NOT EXISTS needed for a data-only migration.

WITH sentinel AS (
    SELECT id FROM projects.projects WHERE project_path = '(no project)'
)
UPDATE sessions.sessions s
   SET project_id = wr.project_id
  FROM workspace.session_registrations wr, sentinel se
 WHERE wr.session_id = s.id
   AND s.project_id = se.id
   AND wr.project_id <> se.id;

WITH sentinel AS (
    SELECT id FROM projects.projects WHERE project_path = '(no project)'
),
session_project_counts AS (
    SELECT r.session_id, r.project_id, COUNT(*) AS request_count
      FROM requests.requests r
      JOIN sessions.sessions s ON s.id = r.session_id
      CROSS JOIN sentinel se
     WHERE s.project_id = se.id
       AND r.project_id <> se.id
     GROUP BY r.session_id, r.project_id
),
winning_project AS (
    SELECT DISTINCT ON (session_id) session_id, project_id
      FROM session_project_counts
     ORDER BY session_id, request_count DESC, project_id ASC
)
UPDATE sessions.sessions s
   SET project_id = wp.project_id
  FROM winning_project wp, sentinel se
 WHERE wp.session_id = s.id
   AND s.project_id = se.id;
