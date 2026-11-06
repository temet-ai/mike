-- `list_model_usage` gained a non-2xx exclusion filter (a request the API
-- rejected is not a model that ran, so it must not appear on the usage
-- page). That filter reads `responses.response_status`, which
-- `idx_responses_request_id_covering` did not carry — so Postgres could
-- no longer answer the join side from the index alone and fell back to a
-- Seq Scan on `responses.responses`, measured 81ms -> 440ms the moment
-- the filter landed. Adding `response_status` to the INCLUDE list
-- restores the Index Only Scan: measured 90.2ms, periodic tier
-- (500ms budget), verdict neutral against the pre-filter query.
--
-- The index is dropped and recreated rather than amended because
-- Postgres has no ALTER INDEX ... INCLUDE. Rebuild cost on the live
-- capture DB is ~500ms at ~207k rows, and migrations apply at startup
-- before the proxy binds its ports, so no concurrent writer is blocked.
--
-- 20260728000001 is left untouched: it is already applied and sqlx
-- refuses to start when an applied migration's checksum changes.

DROP INDEX IF EXISTS responses.idx_responses_request_id_covering;

CREATE INDEX IF NOT EXISTS idx_responses_request_id_covering
    ON responses.responses (request_id)
    INCLUDE (response_status, input_tokens, output_tokens,
             cache_creation_input_tokens, cache_read_input_tokens);
