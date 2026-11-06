-- Composite index for session-scoped, created_at-ordered tool-use reads.
--
-- fetch_tool_uses_for_session (the list_session_tool_uses REST/MCP backing)
-- does `WHERE session_id = $1 ORDER BY created_at, ord OFFSET/LIMIT`. With only
-- the single-column idx_tool_uses_session_id (migration 20260719000001) the
-- planner had to fetch every one of a session's rows and sort them; on the
-- busiest session (7554 tool uses) that was ~71 ms. The composite lets Postgres
-- range-scan the session in created_at/ord order and stop at LIMIT — ~0.6 ms,
-- and it serves both scan directions (btree backward scan for DESC).
--
-- The single-column idx_tool_uses_session_id is a strict prefix of this index,
-- so it is now redundant (COUNT(*) WHERE session_id filters run as an index-only
-- scan on the composite prefix). Dropping it removes the double write-amplification
-- on every tool-use insert.
--
-- IF NOT EXISTS / IF EXISTS: idempotent so a hand-applied environment and a fresh
-- migrate converge to the same state.

CREATE INDEX IF NOT EXISTS idx_tool_uses_session_created_ord
    ON tools.uses (session_id, created_at, ord);

DROP INDEX IF EXISTS tools.idx_tool_uses_session_id;
