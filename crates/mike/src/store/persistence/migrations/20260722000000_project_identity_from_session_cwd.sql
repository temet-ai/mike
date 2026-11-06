-- One automatic project identity: the full canonical Git repository root
-- captured at SessionStart. Request-body path heuristics and the
-- projects.project_aliases redirect table are retired by the accompanying
-- application change.
--
-- `sessions.cwd` is deliberately nullable: historical sessions may predate
-- workspace registration, and a session must remain capturable even when the
-- SessionStart hook cannot provide a working directory. `project_resolution`
-- records who owns the current assignment so a later automatic Git resolution
-- can never overwrite an operator choice.

ALTER TABLE sessions.sessions
    ADD COLUMN IF NOT EXISTS cwd text,
    ADD COLUMN IF NOT EXISTS project_resolution text NOT NULL DEFAULT 'unresolved'
        CHECK (project_resolution IN ('unresolved', 'git', 'operator'));

COMMENT ON COLUMN sessions.sessions.cwd IS
    'Full, verbatim cwd captured at SessionStart. NULL for historical sessions or when no SessionStart cwd was available; never truncated.';

COMMENT ON COLUMN sessions.sessions.project_resolution IS
    'Provenance of sessions.project_id: unresolved = sentinel/legacy assignment, git = canonical repository root resolved from SessionStart cwd, operator = explicit assignment that automatic resolution must not overwrite.';

-- The registration row is currently the only durable source for historical
-- SessionStart cwd values. It is 1:1 with sessions (session_id is its PK), so
-- this backfill is deterministic and preserves the full captured string.
UPDATE sessions.sessions s
   SET cwd = wr.cwd
  FROM workspace.session_registrations wr
 WHERE wr.session_id = s.id
   AND s.cwd IS NULL;

-- Preserve the latest session-summary wire columns and append stable project
-- identity/provenance fields. LEFT JOIN keeps unresolved/corrupt historical
-- rows visible; project_path retains the existing `(no project)` compatibility
-- value while project_id is the honest nullable identity exposed to new code.
CREATE OR REPLACE VIEW mike.session_summary_v AS
SELECT s.session_uuid::text                        AS session_id,
       COALESCE(p.project_path, '(no project)')    AS project_path,
       s.first_seen,
       s.last_seen,
       s.last_status,
       s.title,
       s.pinned,
       COALESCE(p.pinned, FALSE)                   AS project_pinned,
       COUNT(r.id)::int4                           AS request_count,
       COALESCE(mdl.last_models, ARRAY[]::text[])  AS last_models,
       sq.outcome                                  AS session_outcome,
       sq.quality_score                            AS session_quality_score,
       sq.judged_at                                AS session_judged_at,
       s.overview,
       s.overview_generated_at,
       CASE
           WHEN s.project_resolution = 'unresolved'
             OR p.project_path IS NULL
             OR p.project_path = '(no project)' THEN NULL
           ELSE p.id
       END                                         AS project_id,
       s.cwd,
       s.project_resolution
FROM sessions.sessions s
LEFT JOIN projects.projects p ON p.id = s.project_id
LEFT JOIN requests.requests r ON r.session_id = s.id
LEFT JOIN LATERAL (
    SELECT ARRAY_AGG(model ORDER BY first_seen) AS last_models
    FROM (
        SELECT r2.model, MIN(r2.created_at) AS first_seen
        FROM requests.requests r2
        WHERE r2.session_id = s.id
          AND r2.model IS NOT NULL
        GROUP BY r2.model
    ) sub
) mdl ON true
LEFT JOIN mike.session_quality_v sq
       ON sq.session_id = s.session_uuid::text
GROUP BY s.id, p.id, mdl.last_models, sq.outcome, sq.quality_score, sq.judged_at;

COMMENT ON VIEW mike.session_summary_v IS
    'One row per session. Unresolved sessions remain visible with project_path=(no project), project_id NULL, full nullable cwd, and project_resolution provenance. Request count, chronological model list, judge fields, and narrative overview preserve the prior projection semantics.';

-- The old identity mechanisms are removed only after cwd/provenance have been
-- expanded and historical cwd values copied. The full canonical repository
-- root now lives in projects.projects.project_path; basename-only git_repo and
-- alias routing must not participate in identity again.
DROP TABLE IF EXISTS projects.project_aliases;
DROP INDEX IF EXISTS projects.idx_projects_git_repo_unique;
ALTER TABLE projects.projects DROP COLUMN IF EXISTS git_repo;
