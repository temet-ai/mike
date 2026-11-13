-- Covering index for the `/api/tool-uses` cross-session feed
-- (PersistHandle::list_tool_uses, store/persistence/tool_errors.rs).
--
-- That feed pages with `ORDER BY tu.created_at DESC, tu.id DESC OFFSET .. LIMIT ..`
-- over the whole tools.uses table. Without an index on the sort key Postgres
-- SeqScans + Sorts the entire relation (~29k rows and growing monotonically with
-- every captured tool call) to return one 50-row page — measured at 12.3 ms on
-- the live capture DB with a SeqScan(rows=17792) node, which only worsens as the
-- table grows. The .claude/rules/db-performance-evaluation.md bad-node check
-- (read tier) correctly flags this.
--
-- A btree on (created_at DESC, id DESC) lets the planner satisfy the ORDER BY +
-- LIMIT with a bounded index scan (no full sort). The DESC/DESC declaration
-- matches the feed's newest-first default; the ASC variant of the same query is
-- served by a backward scan of the same index. Expand-only, idempotent.
CREATE INDEX IF NOT EXISTS idx_tool_uses_created_at_id
    ON tools.uses (created_at DESC, id DESC);
