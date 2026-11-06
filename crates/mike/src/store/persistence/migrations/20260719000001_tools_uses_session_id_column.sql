-- Add session_id to tools.uses — STEP 1 of 3: column + index, no FK yet.
--
-- Mirrors the tools.results session_id series (20260623000001/…02/…04):
--   …01 (this file, transactional): add the nullable column and its index.
--     No FK constraint and no backfill here — those need either a long
--     statement_timeout lift or a full-table scan, which belong in the
--     companion …02 backfill migration.
--   …02: set-based backfill from requests.requests + FK NOT VALID + VALIDATE.
--   …03: enforce NOT NULL, gated on the Rust write path populating the
--     column on new inserts (mike#40).
--
-- Idempotent: ADD COLUMN IF NOT EXISTS, CREATE INDEX IF NOT EXISTS — safe
-- to re-run (sqlx also guards via _sqlx_migrations, this is belt-and-braces
-- per migrations.rs convention).
--
-- NULLABLE (expand-only): the OLD binary still inserts tools.uses rows
-- without session_id during a deploy cutover; NOT NULL is a later migration
-- after the Rust write path ships the column.

-- ─── Column (metadata-only add, instant on Postgres 12+) ──────────────────
ALTER TABLE tools.uses
    ADD COLUMN IF NOT EXISTS session_id bigint;

COMMENT ON COLUMN tools.uses.session_id IS
    'FK -> sessions.sessions(id) ON DELETE CASCADE (constraint added in the
    companion …02 migration). Denormalized from the carrying request
    (requests.requests.session_id) to enable session-scoped tool-use scans
    without a join. NULLABLE during cutover: the old binary may write rows
    without this column until the Rust write path is updated. Backfilled rows
    are query-usable now; live read queries may only filter on this column
    after the write path populates it on new inserts (new rows are NULL until
    then). NOT NULL enforcement is a follow-up migration after the write path
    ships.';

-- ─── Index (built before the backfill: the backfill''s WHERE session_id IS
-- NULL chunk selector and the post-backfill session-scoped reads both use
-- it; IF NOT EXISTS keeps it idempotent) ──────────────────────────────────
CREATE INDEX IF NOT EXISTS idx_tool_uses_session_id
    ON tools.uses USING btree (session_id);
