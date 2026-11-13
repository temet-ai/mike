-- R4 (issue #26): normalize project identity by git repository.
--
-- The same physical repo accumulates many `project_path` display-name
-- variants (worktrees, different parent dirs, the claude-proxy→temet-proxy
-- rename, the 2-3-segment display-name truncation). This migration adds a
-- canonical grouping key — `git_repo`, the basename of the repository
-- working-directory root — and collapses the existing duplicate project
-- rows onto one canonical identity per repo.
--
-- `git_repo` is captured at runtime from the Claude Code env block
-- (`Is a git repository: true` + the working-directory marker); see
-- `crate::extract::request::extract_git_repo`. It is a LOW-ENTROPY natural
-- key — a directory name, NOT a content hash — so a UNIQUE index on it is
-- legitimate and cheap (cf. `.claude/rules/sql-set-based.md`, which bans
-- sha indexes, not natural-key indexes like the existing
-- `projects_project_path_unique`).
--
-- Known limitation (accepted by operator 2026-06-22): two genuinely
-- different physical repos that share a directory basename (e.g. two repos
-- both named `app`) collapse to one identity under this key. The git remote
-- URL — which would disambiguate — is NOT present in captured traffic, so
-- the basename is the strongest signal available. Edge cases are expected to
-- be reconciled later via a dedicated project-management surface.
--
-- One-shot backfill, `migration` tier. Bounded: the `projects` table holds
-- tens of rows; the four FK-repoint UPDATEs ride the existing
-- `(project_id, ...)` indexes on sessions/requests/operator_turns/lessons.

-- 1. Add the canonical key column (expand — nullable, no table rewrite).
ALTER TABLE projects.projects
    ADD COLUMN IF NOT EXISTS git_repo text;

-- 2. Backfill git_repo for existing rows from the final segment of the
--    stored project_path display name (e.g. 'ai/tools/proxy' → 'proxy').
--    This is the one-time collapse key for legacy data — going forward the
--    value comes from the real git env marker, which agrees with the
--    basename for git-backed projects. The '(no project)' sentinel stays
--    NULL so it is never merged.
UPDATE projects.projects
   SET git_repo = NULLIF(regexp_replace(project_path, '^.*/', ''), '')
 WHERE project_path <> '(no project)'
   AND git_repo IS NULL;

-- 3. Merge duplicate project rows that share a git_repo onto the
--    latest-last_seen canonical row (latest observed path = canonical),
--    repointing every project_id child FK, then deleting the emptied
--    variant rows.
--
--    The four tables below are the ONLY project_id FK references in the
--    schema (verified against all migrations). requests.requests is the one
--    NOT declared ON DELETE CASCADE, so it MUST be repointed before the
--    DELETE or the DELETE would be blocked — repointing all four makes the
--    variant rows childless and the CASCADE/RESTRICT distinction moot.
--
--    The old→canonical mapping is recomputed inline as a CTE in each
--    statement rather than materialised in a temp table: it is
--    transaction-mode-agnostic (a session/autocommit-safe form) and the
--    `first_value(... ORDER BY last_seen DESC, id DESC)` window is
--    deterministic and stable across the statements below (none of them
--    mutate last_seen or id, so canonical_id never shifts mid-merge).
WITH remap AS (
    SELECT id AS old_id,
           first_value(id) OVER (
               PARTITION BY git_repo
               ORDER BY last_seen DESC, id DESC
           ) AS canonical_id
      FROM projects.projects
     WHERE git_repo IS NOT NULL
)
UPDATE sessions.sessions s
   SET project_id = m.canonical_id
  FROM remap m
 WHERE s.project_id = m.old_id
   AND m.old_id <> m.canonical_id;

WITH remap AS (
    SELECT id AS old_id,
           first_value(id) OVER (
               PARTITION BY git_repo ORDER BY last_seen DESC, id DESC
           ) AS canonical_id
      FROM projects.projects WHERE git_repo IS NOT NULL
)
UPDATE requests.requests rq
   SET project_id = m.canonical_id
  FROM remap m
 WHERE rq.project_id = m.old_id
   AND m.old_id <> m.canonical_id;

WITH remap AS (
    SELECT id AS old_id,
           first_value(id) OVER (
               PARTITION BY git_repo ORDER BY last_seen DESC, id DESC
           ) AS canonical_id
      FROM projects.projects WHERE git_repo IS NOT NULL
)
UPDATE memory.operator_turns ot
   SET project_id = m.canonical_id
  FROM remap m
 WHERE ot.project_id = m.old_id
   AND m.old_id <> m.canonical_id;

WITH remap AS (
    SELECT id AS old_id,
           first_value(id) OVER (
               PARTITION BY git_repo ORDER BY last_seen DESC, id DESC
           ) AS canonical_id
      FROM projects.projects WHERE git_repo IS NOT NULL
)
UPDATE memory.lessons l
   SET project_id = m.canonical_id
  FROM remap m
 WHERE l.project_id = m.old_id
   AND m.old_id <> m.canonical_id;

-- Roll the earliest first_seen across the merged set onto the surviving
-- canonical row (which already holds the latest last_seen by construction).
UPDATE projects.projects p
   SET first_seen = agg.min_first_seen
  FROM (
      SELECT git_repo, min(first_seen) AS min_first_seen
        FROM projects.projects
       WHERE git_repo IS NOT NULL
       GROUP BY git_repo
      HAVING count(*) > 1
  ) agg
 WHERE p.git_repo = agg.git_repo;

-- Delete the now-childless variant rows (every non-canonical row sharing a
-- git_repo with a later-seen sibling).
WITH remap AS (
    SELECT id AS old_id,
           first_value(id) OVER (
               PARTITION BY git_repo ORDER BY last_seen DESC, id DESC
           ) AS canonical_id
      FROM projects.projects WHERE git_repo IS NOT NULL
)
DELETE FROM projects.projects p
 USING remap m
 WHERE p.id = m.old_id
   AND m.old_id <> m.canonical_id;

-- 4. Enforce one project row per git_repo going forward. Partial so the
--    NULL git_repo rows (the '(no project)' sentinel and any path-only
--    projects) are exempt and may coexist.
CREATE UNIQUE INDEX IF NOT EXISTS idx_projects_git_repo_unique
    ON projects.projects (git_repo)
    WHERE git_repo IS NOT NULL;
