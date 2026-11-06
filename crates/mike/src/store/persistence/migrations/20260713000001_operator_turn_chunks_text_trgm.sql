-- The /sessions content-search route is moving off a brute-force ILIKE scan
-- over requests.requests (165k+ rows, times out at the 30s HTTP deadline) onto
-- the already-populated memory.operator_turn_chunks corpus (47.7k rows). That
-- table has no index supporting substring search on chunk_text: the join side
-- (turn_id) is already covered by idx_otc_turn, but a literal
-- `chunk_text ILIKE '%needle%'` was a full Seq Scan (~700-850ms on real data,
-- see commit Perf block).
--
-- pg_trgm is already enabled (20260604000001_baseline.sql, used by
-- idx_sessions_title_trgm / idx_tool_uses_bash_command_trgm) — CREATE
-- EXTENSION IF NOT EXISTS is idempotent so it is safe to restate here rather
-- than assume.
--
-- Plain (non-CONCURRENTLY) CREATE INDEX, matching the established convention
-- for this repo's small/medium tables (e.g. 20260625000001_judge_labeling.sql,
-- 20260623000005_tools_results_dedup.sql): sqlx runs each migration in its own
-- transaction and CONCURRENTLY cannot run inside one; at 47.7k rows the build
-- completes in well under a second and there are no active writers during the
-- startup migration window, so CONCURRENTLY's extra complexity (no-transaction
-- directive, retry-on-invalid-index handling) buys nothing here.
--
-- No index added on the operator_turns join side: idx_otc_turn (turn_id) and
-- operator_turns_pkey (id) already cover the JOIN t.id = c.turn_id, and the
-- query has no predicate on operator_turns columns to filter — GROUP BY
-- t.session_id runs over exactly the rows the trgm index already narrowed
-- down to, so a second index there would have nothing new to speed up.

CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE INDEX IF NOT EXISTS idx_otc_chunk_text_trgm
    ON memory.operator_turn_chunks USING gin (chunk_text gin_trgm_ops);

COMMENT ON INDEX memory.idx_otc_chunk_text_trgm IS
    'GIN trigram index backing literal ILIKE ''%needle%'' substring search over '
    'chunk_text for the /sessions content-search route. Trigram matching needs a '
    'literal of at least 3 characters to extract a usable trigram; a 1-2 character '
    'needle cannot use this index and falls back to a Seq Scan regardless of the '
    'API-level minimum-length enforced by the frontend/route.';
