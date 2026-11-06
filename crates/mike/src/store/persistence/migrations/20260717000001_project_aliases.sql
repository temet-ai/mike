-- Alias table backing project merge: every path folded into a target
-- project keeps a row here so capture-side get-or-create redirects the
-- alias to the target instead of resurrecting the merged source row.
-- The in-memory alias map (PersistHandle.project_aliases) is loaded from
-- this table at startup and refreshed after each merge.
CREATE TABLE IF NOT EXISTS projects.project_aliases (
    alias_path text PRIMARY KEY,
    project_id bigint NOT NULL REFERENCES projects.projects(id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now()
);

-- Supports the merged_paths aggregate join in list_projects and the
-- alias chain-fold UPDATE in merge_projects (both filter by project_id).
CREATE INDEX IF NOT EXISTS idx_project_aliases_project_id
    ON projects.project_aliases (project_id);
