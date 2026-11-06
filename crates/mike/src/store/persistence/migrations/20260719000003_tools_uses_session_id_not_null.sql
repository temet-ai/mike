-- tools.uses.session_id NOT NULL enforcement — STEP 3 of 3.
--
-- Prerequisites (must already be applied live before this runs):
--   …01: ADD COLUMN tools.uses.session_id (nullable) + btree index
--   …02: backfill from requests.requests + FK NOT VALID + VALIDATE
--   Rust write path (mike#40): tools.uses inserts populate session_id on
--   every new row — this migration must not land before that ships, or
--   any insert still missing the column violates NOT NULL.
--
-- This migration:
--   1. Lifts the pool's 30-s statement_timeout (the backfill UPDATE is
--      mutation-safe but may touch residual NULL rows written between the
--      first backfill and the Rust write-path deploy).
--   2. Set-based idempotent backfill of any remaining NULLs.
--   3. ALTER COLUMN ... SET NOT NULL — instant on Postgres 12+ (checks
--      pg_constraint metadata only, no table scan) when the column has no
--      remaining NULLs. If any NULL slips through step 2 the ALTER will
--      fail, which is the correct and safe behaviour.
--
-- Idempotency:
--   - The UPDATE WHERE session_id IS NULL is a no-op if the column is
--     already fully populated (re-run after full population = 0 rows updated).
--   - SET NOT NULL is idempotent: re-applying it when the constraint already
--     exists is a no-op in Postgres (no error).
--   - sqlx's _sqlx_migrations guard also prevents double-apply in normal
--     operation; the above is belt-and-braces for hand-apply safety.
--
-- TRANSACTIONAL (no `-- no-transaction` directive): the residual backfill
-- here is expected to touch only rows written in the gap since …02 ran.

-- ─── 1. Lift statement_timeout for this transaction only ───────────────────
SET LOCAL statement_timeout = 0;

-- ─── 2. Backfill remaining NULLs (set-based, idempotent) ──────────────────
-- Joins tools.uses (the probe side) to requests.requests (the build side)
-- via Hash Join. The existing idx_tool_uses_session_id index and the WHERE
-- session_id IS NULL clause together ensure only un-filled rows are touched.
-- A re-run after full population processes 0 rows.
UPDATE tools.uses u
    SET session_id = r.session_id
FROM requests.requests r
WHERE u.request_id = r.id
  AND u.session_id IS NULL;

-- ─── 3. Enforce NOT NULL ───────────────────────────────────────────────────
-- On Postgres 12+ this is a metadata-only change (no table scan) when no
-- NULLs remain — the update in step 2 above guarantees that. Idempotent: a
-- second apply of SET NOT NULL on an already-NOT-NULL column is silently
-- accepted by Postgres.
ALTER TABLE tools.uses
    ALTER COLUMN session_id SET NOT NULL;
