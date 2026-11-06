-- Add session_id to tools.results — STEP 1 of 2: column + index, no FK yet.
--
-- Restructured into two files (expand-contract, idempotent):
--   …01 (this file, transactional): add the nullable column and its index.
--     No FK constraint and no backfill here — those need either a long
--     statement_timeout lift or per-batch commits, which belong in the
--     no-transaction companion file …02.
--   …02 (-- no-transaction): batched idempotent backfill (per-chunk COMMIT)
--     followed by the guarded FK ADD CONSTRAINT … NOT VALID + VALIDATE.
--
-- Why the split: sqlx wraps each migration file in ONE transaction by
-- default, so a batched backfill loop in a normal migration could not
-- COMMIT between chunks — it would hold one long lock and accumulate WAL,
-- defeating the point of batching. sqlx 0.8 honours a `-- no-transaction`
-- first-line directive (verified in sqlx-core 0.8.6 source.rs:127, embedded
-- by the migrate! macro), so …02 runs outside a transaction and its
-- per-batch COMMITs actually release locks/WAL. This file stays
-- transactional: ADD COLUMN and CREATE INDEX are cheap and atomic.
--
-- Idempotent: ADD COLUMN IF NOT EXISTS, CREATE INDEX IF NOT EXISTS — safe
-- to re-run (sqlx also guards via _sqlx_migrations, this is belt-and-braces
-- per migrations.rs convention).
--
-- NULLABLE (expand-only): the OLD binary still inserts tools.results rows
-- without session_id during a deploy cutover; NOT NULL is a later migration
-- after the Rust write path ships the column.

-- ─── Column (metadata-only add, instant on Postgres 12+) ──────────────────
ALTER TABLE tools.results
    ADD COLUMN IF NOT EXISTS session_id bigint;

COMMENT ON COLUMN tools.results.session_id IS
    'FK -> sessions.sessions(id) ON DELETE CASCADE (constraint added in the
    companion …02 migration). Denormalized from the carrying request
    (requests.requests.session_id) to enable session-scoped tool-result scans
    without a join. NULLABLE during cutover: the old binary may write rows
    without this column until the Rust write path is updated. Backfilled rows
    are query-usable now; live read queries may only filter on this column
    after the write path populates it on new inserts (new rows are NULL until
    then). NOT NULL enforcement is a follow-up migration after the write path
    ships.';

-- ─── Index (built before the backfill: the backfill''s WHERE session_id IS
-- NULL chunk selector and the post-backfill session-scoped reads both use
-- it; IF NOT EXISTS keeps it idempotent) ──────────────────────────────────
CREATE INDEX IF NOT EXISTS idx_tool_results_session_id
    ON tools.results USING btree (session_id);
