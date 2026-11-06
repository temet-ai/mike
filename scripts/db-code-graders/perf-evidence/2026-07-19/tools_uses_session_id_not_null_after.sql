-- Perf evidence for migration 20260719000003_tools_uses_session_id_not_null.
-- Measured on live data (:6666), mutation-safe in a rolled-back transaction:
--   residual backfill 278 rows           33.5 ms
--   ALTER ... SET NOT NULL (156,777 rows) 119.2 ms  (full-table verify scan)
--   total                                ~153 ms     (migration budget 5000 ms)
-- The ALTER succeeded, confirming 0 NULL session_id rows remain post-backfill.
--
-- The two statements below are the migration's measured operations verbatim.

-- 1. Residual backfill of any tools.uses rows written in the gap between the
--    000002 bulk backfill and the write-path deploy.
UPDATE tools.uses u
    SET session_id = r.session_id
FROM requests.requests r
WHERE u.request_id = r.id
  AND u.session_id IS NULL;

-- 2. Enforce NOT NULL — metadata change plus a one-time full-table verify scan.
ALTER TABLE tools.uses
    ALTER COLUMN session_id SET NOT NULL;
