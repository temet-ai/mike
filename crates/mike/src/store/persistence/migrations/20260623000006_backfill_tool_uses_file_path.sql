-- Backfill tools.uses.file_path for historical rows.
-- The capture write path never populated this column; extract it from the
-- already-stored JSONB input where present.  Idempotent: the `file_path IS NULL`
-- guard makes re-running a no-op.  Set-based, no correlated subquery.
UPDATE tools.uses
SET file_path = input->>'file_path'
WHERE file_path IS NULL
  AND input ? 'file_path';
