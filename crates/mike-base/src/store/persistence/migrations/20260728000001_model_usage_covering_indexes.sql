-- `PersistHandle::list_model_usage` (per-model token usage for the
-- usage/costs settings page) aggregates `requests.requests` JOIN
-- `responses.responses` over a trailing-days window. Both tables carry
-- large captured-body columns (request_body/filtered_request_body/
-- sent_patch_bytes/params/request_headers on requests; response_body/
-- response_headers/token_usage/stop_details on responses), so a plain
-- Seq Scan on either heap means paying for gigabytes of body bytes to
-- read three small columns. Measured 2026-07-28: baseline (no covering
-- index) 574-636ms on the live capture DB for a 30-day window — both
-- sides Seq Scan (requests: ~40% selectivity on `created_at`, correctly
-- not BRIN-selective per the planner; responses: no filter at all,
-- every row read to find its paired request).
--
-- A `created_at`-keyed covering index on `requests.requests` (id, model
-- INCLUDEd) turns its scan into an Index Only Scan — the planner no
-- longer needs the heap at all for this query's projection, only the
-- narrow index. A `request_id`-keyed covering index on
-- `responses.responses` (the four GENERATED STORED token columns
-- INCLUDEd) does the same for the unfiltered join side. Combined:
-- 574-636ms -> ~81-88ms (measured with EXPLAIN (ANALYZE, BUFFERS),
-- best-of-3 via scripts/db-perf-check, fully warm shared_buffers, zero
-- Seq Scan / bad-node flags remaining). Still over the 50ms read-tier
-- budget — the residual cost is row-processing (hash join + parallel
-- aggregate over ~140k rows), not I/O, so a further fix is a
-- materialized rollup, not another index; tracked as a documented
-- perf-tier exception rather than solved here.
--
-- Write-path: both leading columns are effectively append-only
-- (`created_at` monotonic; `responses.responses` rows are written once,
-- in request-creation order, at snapshot_terminal time), so every
-- insert lands on the rightmost B-tree leaf — the cheap case, not the
-- random-leaf pattern this repo bans sha indexes for. Measured
-- 2026-07-28: the `requests.requests` insert path is 3.0ms with the
-- index vs 3.9ms without (noise, not a regression); the combined
-- mint-request+insert-response path is 3.5-3.8ms with vs without the
-- responses index — both comfortably inside the 5ms hot-tier budget
-- either way.
--
-- Plain (non-CONCURRENTLY) CREATE INDEX, matching this repo's
-- established convention for a table this size (see
-- 20260716010000_system_prompt_firing_rules_request_index.sql):
-- migrations apply at startup before the proxy binds its ports, so
-- there are no concurrent writers to block. Measured build time on the
-- live capture DB (~220-230k rows / ~140k rows respectively): 426ms for
-- the requests index, 500ms for the responses index.

CREATE INDEX IF NOT EXISTS idx_requests_created_at_covering
    ON requests.requests (created_at) INCLUDE (id, model);

CREATE INDEX IF NOT EXISTS idx_responses_request_id_covering
    ON responses.responses (request_id)
    INCLUDE (input_tokens, output_tokens, cache_creation_input_tokens, cache_read_input_tokens);
