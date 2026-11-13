-- Add session_id to tools.results — STEP 2 of 2: backfill + FK (idempotent).
--
-- TRANSACTIONAL (no `-- no-transaction` directive). See the long note below:
-- per-batch-COMMIT batching is NOT achievable through sqlx 0.8's migrate!
-- runner, so this is the honest one-shot set-based backfill with the pool''s
-- statement_timeout lifted for the duration — measured at ~4-7 s for 2.3M
-- rows, well within a one-shot startup migration''s budget.
--
-- ── Why not batched-with-COMMIT (the rejected approach) ───────────────────
-- The brief asked for per-batch COMMITs so locks/WAL release between chunks.
-- That requires running OUTSIDE a transaction. sqlx 0.8 DOES honour a
-- `-- no-transaction` first-line directive (verified: sqlx-core 0.8.6
-- source.rs:127, embedded by the migrate! macro), which drops sqlx''s OWN
-- wrapper transaction. BUT sqlx then sends the whole migration body to
-- Postgres as a SINGLE simple-query batch (sqlx-postgres 0.8.6 migrate.rs:280
-- — `conn.execute(&migration.sql)`), and Postgres treats a multi-statement
-- simple-query as an IMPLICIT transaction block. `COMMIT` inside an implicit
-- block raises SQLSTATE 2D000 `invalid transaction termination`. This was
-- not theoretical: a no-transaction file with a CALLed PL/pgSQL procedure
-- doing per-chunk COMMIT was built and run through the real migrator, and it
-- failed with exactly that error during template build. (no_tx works for a
-- SINGLE self-committing statement like CREATE INDEX CONCURRENTLY; it does
-- NOT enable a multi-statement COMMIT loop.)
--
-- Per .claude/rules/scope-and-agents.md ("Describe-A-ship-~A" is forbidden)
-- and the brief''s own instruction not to fake batching with a loop that
-- can''t commit, the honest alternative is this one-shot UPDATE. The
-- trade-off: it holds row locks on the updated rows and writes WAL for the
-- whole 2.3M-row change in one transaction. At this table size (~2.3M rows,
-- ~1.3 s for the join scan, ~4-7 s with WAL) that is acceptable for a
-- one-time startup migration. If this table grows large enough that a
-- single-transaction backfill becomes a startup hazard, the correct fix is a
-- background backfill WORKER (issuing its own per-chunk transactions over a
-- normal pooled connection) — NOT a no_tx migration, which cannot batch.
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
-- backfill UPDATE + VALIDATE scan exceed 30s comfort margin on 2.3M rows).
-- SET LOCAL is scoped to this migration transaction. Operator approval for
-- the longer migration timeout is recorded in the task brief (2026-06-23).
SET LOCAL statement_timeout = 0;
SET LOCAL lock_timeout = '60s';

-- ─── Backfill: set-based UPDATE, only un-filled rows (resumable) ───────────
-- Plan: Hash Join req.id = r.request_id (requests builds the hash, ~58k rows;
-- tools.results probes, ~2.3M). No per-row subquery. WHERE session_id IS NULL
-- means a re-run after a partial hand-application redoes only what is missing.
UPDATE tools.results r
SET session_id = req.session_id
FROM requests.requests req
WHERE req.id = r.request_id
  AND r.session_id IS NULL;

-- ─── FK constraint: NOT VALID (metadata-only add), guarded ────────────────
DO $guard$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'tool_results_session_id_fkey'
          AND conrelid = 'tools.results'::regclass
    ) THEN
        ALTER TABLE tools.results
            ADD CONSTRAINT tool_results_session_id_fkey
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
        WHERE conname = 'tool_results_session_id_fkey'
          AND conrelid = 'tools.results'::regclass
          AND NOT convalidated
    ) THEN
        ALTER TABLE tools.results
            VALIDATE CONSTRAINT tool_results_session_id_fkey;
    END IF;
END;
$validate$;
