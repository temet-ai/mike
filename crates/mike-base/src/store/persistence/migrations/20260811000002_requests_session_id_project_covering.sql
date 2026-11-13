-- GitHub issue #32: `list_sessions_paginated` and its siblings need each
-- session's distinct `requests.requests.project_id` set (the session-level
-- `project_id` can diverge from individual requests' — nested-repo `cd` /
-- subagent work under a parent workspace cwd). The bare `idx_requests_session_id
-- (session_id)` index cannot answer that without a heap fetch per row (`project_id`
-- isn't in any index), which measured as a real regression via
-- scripts/db-perf-check: 18.6ms baseline -> 104.9ms with a naive per-session
-- LATERAL scan, over the 50ms read-tier budget.
--
-- Widening the index to INCLUDE (project_id) lets the planner answer
-- `WHERE session_id = ... ` with an Index Only Scan that also carries
-- project_id, no second index needed alongside it — the covering index is a
-- strict superset of the bare one for every existing query filtering on
-- `session_id` alone (INCLUDE columns are payload-only, not part of the sort
-- key). Combined with folding the new aggregate into the existing
-- per-session request_count LATERAL (one scan of requests.requests instead
-- of two — see rehydrate.rs), measured 2026-08-11 via db-perf-check
-- (best of 15, live capture DB): 19.3ms -> 21.2ms, verdict neutral, both
-- comfortably under the 50ms read-tier budget.
--
-- Dropped and recreated rather than amended because Postgres has no
-- ALTER INDEX ... INCLUDE. Plain (non-CONCURRENTLY) CREATE INDEX, matching
-- this repo's established convention for this table (see
-- 20260728000001_model_usage_covering_indexes.sql /
-- 20260729000001_model_usage_covering_index_add_status.sql): migrations
-- apply at startup before the proxy binds its ports, so there are no
-- concurrent writers to block. Measured rebuild time on the live capture DB
-- (~243k rows): well under 2s for the drop+create pair.

DROP INDEX IF EXISTS requests.idx_requests_session_id;

CREATE INDEX IF NOT EXISTS idx_requests_session_id
    ON requests.requests (session_id) INCLUDE (project_id);
