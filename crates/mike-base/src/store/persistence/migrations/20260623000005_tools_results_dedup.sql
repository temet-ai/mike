-- tools.results dedup: keep one row per tool_use_id (the original / lowest-id),
-- delete ~2.3M duplicate rows that accumulated because the old NOT EXISTS guard
-- checked (request_id, ord) instead of tool_use_id — every re-quote of a
-- tool_result in a follow-up request inserted a new row.
--
-- Three steps:
--   1. DELETE duplicates via a set-based Hash Join (one aggregate scan +
--      one delete pass — O(N), not O(N²)):
--        build phase: GROUP BY tool_use_id → MIN(id) for each of 58k distinct uses.
--        probe phase: delete every row whose id is NOT the min for its tool_use_id.
--      EXPLAIN plan (from live DB): Hash Join (cost=402k..798k), two Seq Scans.
--      No correlated subquery; no per-row index probes.
--
--   2. CREATE UNIQUE INDEX IF NOT EXISTS uq_tool_results_tool_use_id — enforces
--      one-row-per-use going forward and enables ON CONFLICT (tool_use_id) DO NOTHING
--      in the write path.  tool_use_id is a surrogate bigint FK (tools.uses(id)),
--      NOT a content hash — indexing and UNIQUE-constraining it is correct and
--      does NOT violate the no-sha-index rule.
--
--   3. Promote the UNIQUE index to a named pg_constraint entry (idempotent via
--      DO block), for \d visibility and schema introspection.
--
-- Idempotency:
--   - After dedup, the DELETE's build phase finds 0 rows where id > min_id.
--   - CREATE UNIQUE INDEX IF NOT EXISTS is a no-op if the index already exists.
--   - The DO block in step 3 is guarded by a pg_constraint existence check.
--   - A double-apply is safe.
--
-- Startup-gate timing (EXPLAIN plan on 2.36M-row / ~9GB table, from live DB):
--   Plan: Hash Join; two sequential scans of tools.results.
--   With data in page cache: ~15-30s expected.
--   Cold I/O: up to 60-90s depending on server I/O bandwidth.
--   RECOMMEND running step 1 manually before the next deploy if the DB is cold:
--
--     psql "$PROXY_DATABASE_URL" -c "
--       SET statement_timeout = 0;
--       DELETE FROM tools.results d
--       USING (SELECT tool_use_id, MIN(id) AS min_id FROM tools.results GROUP BY tool_use_id) keep
--       WHERE d.tool_use_id = keep.tool_use_id AND d.id > keep.min_id;"
--
--   After the manual step this migration's DELETE processes 0 rows and completes
--   in milliseconds.  The UNIQUE index creation then runs on 58k rows (fast).
--
-- IMPORTANT — no-transaction required for two reasons:
--   1. CREATE UNIQUE INDEX (not CONCURRENTLY) cannot run inside a transaction.
--   2. SET statement_timeout must be session-level (no transaction to scope to).
--
-- no-transaction

-- Lift statement_timeout for the DELETE.
SET statement_timeout = 0;

-- ─── Step 1: Dedup DELETE (set-based, O(N) via Hash Join) ─────────────────
-- Build side: aggregate once → (tool_use_id, MIN(id)) for all 58k distinct uses.
-- Probe side: delete every row where id > min_id for its tool_use_id.
-- Hash Join: O(M log M) build on 58k rows + O(N) probe on 2.36M rows.
-- No correlated subquery; no per-row index probes on the inner side.
-- Idempotent: after dedup, WHERE id > min_id matches 0 rows.
DELETE FROM tools.results d
USING (
    SELECT tool_use_id, MIN(id) AS min_id
    FROM tools.results
    GROUP BY tool_use_id
) keep
WHERE d.tool_use_id = keep.tool_use_id
  AND d.id > keep.min_id;

-- ─── Step 2: UNIQUE index on tool_use_id ──────────────────────────────────
-- Cannot run inside a transaction — requires no-transaction directive above.
-- Non-CONCURRENTLY: safe at migration time (no active writers during startup).
-- After dedup there is exactly one row per tool_use_id so the index builds
-- cleanly on 58k rows.
-- This also enables ON CONFLICT (tool_use_id) DO NOTHING in the write path —
-- Postgres resolves the conflict target by column name against the UNIQUE index;
-- a named pg_constraint entry is not required for ON CONFLICT to work.
CREATE UNIQUE INDEX IF NOT EXISTS uq_tool_results_tool_use_id
    ON tools.results (tool_use_id);

-- ─── Step 3: Promote index to named constraint (idempotent DO block) ───────
-- Makes the constraint visible in pg_constraint / \d output for introspection.
-- The DO block is idempotent: no-op if the constraint already exists.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'uq_tool_results_tool_use_id'
          AND conrelid = 'tools.results'::regclass
    ) THEN
        ALTER TABLE tools.results
            ADD CONSTRAINT uq_tool_results_tool_use_id
            UNIQUE USING INDEX uq_tool_results_tool_use_id;
    END IF;
END;
$$;
