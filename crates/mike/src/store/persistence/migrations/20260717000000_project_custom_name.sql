-- Operator-set friendly name for a project, distinct from the
-- capture-derived display_name (see extract::request::project_display_name).
-- Never written by capture upserts or canonicalization.
ALTER TABLE projects.projects ADD COLUMN IF NOT EXISTS custom_name text;
