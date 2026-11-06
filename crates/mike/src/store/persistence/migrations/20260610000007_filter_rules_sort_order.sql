-- Add operator-controlled, persisted rule ordering to filter rules
-- (rules.rules — the table behind the /api filter_rules surface).
--
-- Background: the filter engine orders rules by (updated_at ASC, id ASC),
-- so EDITING a rule reshuffles its application order. That is now wrong:
-- slice-level rules compose sequentially per block, and the operator will
-- drag-reorder rules in the workbench UI. The order itself becomes
-- operator-owned state and must be persisted.
--
-- Expand-only: one new column, no drops, no renames. The Rust consumption
-- (engine ORDER BY, max+1 on insert, reorder endpoint) is a separate task.
--
-- Decisions, documented per db-dev conventions:
--   - Column name `sort_order` ("order" is a reserved word).
--   - Backfill = row_number() OVER (updated_at ASC, id ASC) so the
--     cutover preserves today's effective ordering exactly.
--   - DEFAULT = nextval on a dedicated sequence, seeded past the
--     backfilled max. The Rust service will set max+1 explicitly; the
--     sequence is a safe fallback only — any row inserted without an
--     explicit sort_order lands strictly after everything backfilled,
--     never at position 0 / ambiguously interleaved. It is NOT
--     load-bearing and may drift ahead of the live max; that only makes
--     fallback rows sort later, which is the intended failure mode.
--   - NO uniqueness constraint, by choice: an immediate UNIQUE breaks
--     multi-row swap/reorder UPDATEs mid-statement, and even a
--     DEFERRABLE one turns concurrent "insert at max+1" races into
--     commit-time errors on the rule-create path. Duplicates are
--     harmless because every consumer must tie-break deterministically:
--     ORDER BY sort_order ASC, id ASC. The reorder endpoint owns
--     compaction.
--   - No index: rules.rules is a tiny config table (dozens of rows);
--     per convention small tables need only the PK, and sort_order is
--     not an FK.
--   - Single-file backfill + SET NOT NULL is safe here (no expand/
--     contract split needed): the table is config-sized, the full-scan
--     NOT NULL validation is microseconds, and nothing writes to it
--     concurrently during startup migrations.
--
-- Table-size note: this is NOT the requests-table backfill failure mode
-- (no request_body parsing, no large scan — see the 120 s startup-gate
-- incident); the whole table fits in one page in practice.

-- Step 1: expand — add the column nullable so existing rows pass.
ALTER TABLE rules.rules ADD COLUMN IF NOT EXISTS sort_order integer;

-- Step 2: backfill from the engine's current effective order, so the
-- cutover is semantics-preserving. Guarded on IS NULL for idempotency:
-- a re-run never reshuffles rows that already have an order.
UPDATE rules.rules r
SET sort_order = ranked.rn
FROM (
    SELECT id, row_number() OVER (ORDER BY updated_at ASC, id ASC) AS rn
    FROM rules.rules
) ranked
WHERE r.id = ranked.id
  AND r.sort_order IS NULL;

-- Step 3: lock in NOT NULL (safe single-file sequence — see header).
ALTER TABLE rules.rules ALTER COLUMN sort_order SET NOT NULL;

-- Step 4: fallback DEFAULT for rows inserted without an explicit
-- sort_order. Sequence is owned by the column (dropped with it) and
-- seeded one past the backfilled max so fallback rows append at the end.
CREATE SEQUENCE IF NOT EXISTS rules.rules_sort_order_seq
    AS integer
    OWNED BY rules.rules.sort_order;

SELECT setval(
    'rules.rules_sort_order_seq',
    (SELECT coalesce(max(sort_order), 0) + 1 FROM rules.rules),
    false  -- is_called = false: the next nextval() RETURNS this value
);

ALTER TABLE rules.rules
    ALTER COLUMN sort_order SET DEFAULT nextval('rules.rules_sort_order_seq');

COMMENT ON COLUMN rules.rules.sort_order IS
    'Operator-controlled application order (ascending). Consumers MUST order by (sort_order ASC, id ASC) — duplicates are permitted by design (no UNIQUE: it would break swap/reorder UPDATEs) and the id tie-break keeps ordering deterministic. New rows: the service sets max+1 explicitly; the sequence DEFAULT is a non-load-bearing fallback that appends at the end. Backfilled 2026-06-10 from row_number() OVER (updated_at ASC, id ASC) to preserve the pre-existing engine order.';
