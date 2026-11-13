-- Add optional project_path column to filter rules.
-- NULL = global rule (fires on every request).
-- Non-NULL = scoped rule (fires only when the request's project path matches exactly).
ALTER TABLE rules.rules ADD COLUMN IF NOT EXISTS project_path TEXT;
