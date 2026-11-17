-- A location can move (its root_path changes, its location_uuid stays), a worktree
-- names the main checkout it belongs to, and a root missing on disk is marked gone
-- rather than removed. The machine that holds a location sets parent_location_uuid,
-- gone_at and present_at on each knowledge scan; created_at lets that scan tell a
-- folder registered after a checkout disappeared from one that was already there.
SET LOCAL lock_timeout = '5s';
ALTER TABLE projects.locations
    ADD COLUMN IF NOT EXISTS parent_location_uuid uuid
        REFERENCES projects.locations(location_uuid) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS gone_at timestamptz,
    ADD COLUMN IF NOT EXISTS present_at timestamptz,
    ADD COLUMN IF NOT EXISTS created_at timestamptz NOT NULL DEFAULT now();
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint
                    WHERE conname = 'locations_parent_not_self'
                      AND conrelid = 'projects.locations'::regclass) THEN
        ALTER TABLE projects.locations ADD CONSTRAINT locations_parent_not_self
            CHECK (parent_location_uuid IS DISTINCT FROM location_uuid);
    END IF;
END $$;
CREATE INDEX IF NOT EXISTS locations_parent ON projects.locations (parent_location_uuid)
    WHERE parent_location_uuid IS NOT NULL;
