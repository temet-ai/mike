-- Restore the UNIQUE constraint on projects.projects(project_path).
--
-- project_path is a legitimate natural key (low-entropy text, one row
-- per filesystem path — NOT a content hash). The constraint was lost
-- in the migration squash (2026-06-02) and its absence causes:
--
--   1. ON CONFLICT (project_path) in test helpers and production
--      get-or-create paths to error with "no unique or exclusion
--      constraint matching the ON CONFLICT specification".
--   2. A latent TOCTOU race in production's WHERE NOT EXISTS
--      get-or-create path — two concurrent captures for the same
--      project_path can both pass the EXISTS check and insert two rows.
--
-- No duplicate project_path values exist on the live DB (confirmed
-- before this migration was written), so this is a pure expand — no
-- dedup or backfill needed.
--
-- The UNIQUE constraint implicitly creates a B-tree index, so a
-- separate CREATE INDEX is not needed (and would be redundant).
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
         WHERE conname = 'projects_project_path_unique'
           AND conrelid = 'projects.projects'::regclass
    ) THEN
        ALTER TABLE projects.projects
            ADD CONSTRAINT projects_project_path_unique UNIQUE (project_path);
    END IF;
END $$;
