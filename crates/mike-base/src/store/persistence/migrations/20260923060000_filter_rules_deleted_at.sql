ALTER TABLE rules.rules ADD COLUMN IF NOT EXISTS deleted_at timestamptz NULL;
