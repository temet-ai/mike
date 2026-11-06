-- A project the operator folds into another (a worktree or a second clone that
-- was captured as its own project) keeps its row, pointing at the project it
-- now belongs to. Listings hide it and path lookups follow the pointer.

ALTER TABLE projects.projects
    ADD COLUMN merged_into_id bigint REFERENCES projects.projects(id),
    ADD COLUMN merged_at timestamptz,
    ADD CONSTRAINT projects_not_merged_into_self CHECK (merged_into_id IS NULL OR merged_into_id <> id);

CREATE INDEX projects_merged_into ON projects.projects (merged_into_id) WHERE merged_into_id IS NOT NULL;
