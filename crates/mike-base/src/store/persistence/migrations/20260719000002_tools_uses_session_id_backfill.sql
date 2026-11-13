-- Add session_id to tools.uses — STEP 2 of 3: backfill + FK (idempotent).
--
-- Mirrors 20260623000002 (tools.results). TRANSACTIONAL (no
-- `-- no-transaction` directive) — per-batch-COMMIT batching is not
-- achievable through sqlx 0.8's migrate! runner (see the tools.results
-- companion migration for the full rationale: a no-transaction file is sent
-- to Postgres as one simple-query batch, which Postgres treats as an
-- implicit transaction block that rejects COMMIT). This is the honest
-- one-shot set-based backfill with the pool''s statement_timeout lifted for
-- the duration.
--
-- ── Idempotency ───────────────────────────────────────────────────────────
-- sqlx records the version only after the whole file commits, and the file
-- is transactional, so a failure rolls the whole thing back — a re-run starts
-- clean. The guards below are belt-and-braces (matching migrations.rs
-- convention) so the file is ALSO safe to apply by hand on a partially-
-- migrated DB:
--   - backfill touches only WHERE session_id IS NULL (a re-run resumes);
--   - FK ADD is guarded on pg_constraint absence;
--   - VALIDATE is guarded on NOT convalidated.

-- Lift the pool''s 30s statement_timeout for this transaction only (the
-- backfill UPDATE + VALIDATE scan may exceed the 30s comfort margin at
-- tools.uses'' current row count).
SET LOCAL statement_timeout = 0;
SET LOCAL lock_timeout = '60s';

-- ─── Backfill: set-based UPDATE, only un-filled rows (resumable) ───────────
-- Plan: Hash Join req.id = u.request_id (requests builds the hash side);
-- tools.uses probes. No per-row subquery. WHERE session_id IS NULL means a
-- re-run after a partial hand-application redoes only what is missing.
UPDATE tools.uses u
SET session_id = req.session_id
FROM requests.requests req
WHERE req.id = u.request_id
  AND u.session_id IS NULL;

-- ─── FK constraint: NOT VALID (metadata-only add), guarded ────────────────
DO $guard$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'tool_uses_session_id_fkey'
          AND conrelid = 'tools.uses'::regclass
    ) THEN
        ALTER TABLE tools.uses
            ADD CONSTRAINT tool_uses_session_id_fkey
                FOREIGN KEY (session_id)
                REFERENCES sessions.sessions(id)
                ON DELETE CASCADE
                NOT VALID;
    END IF;
END;
$guard$;

-- ─── VALIDATE the constraint (single scan, ShareUpdateExclusiveLock), guarded ─
DO $validate$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'tool_uses_session_id_fkey'
          AND conrelid = 'tools.uses'::regclass
          AND NOT convalidated
    ) THEN
        ALTER TABLE tools.uses
            VALIDATE CONSTRAINT tool_uses_session_id_fkey;
    END IF;
END;
$validate$;
