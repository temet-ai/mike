-- `requests.system_prompt_firing_rules.request_system_prompt_id` is an FK to
-- `requests.system_prompts(id)` (issue #225) but carried no index. Every
-- caller that joins from a system_prompts row out to its firing rules —
-- both LATERAL array_aggs and both NOT EXISTS anti-joins in
-- PersistHandle::list_anomalies_windowed — filters on this column, so the
-- planner fell back to a Seq Scan of the whole table (172k+ rows) per
-- outer row. EXPLAIN (ANALYZE, BUFFERS) on the isolated LATERAL confirmed
-- this was the sole clause the query never finished past a 45s
-- statement_timeout; the date-window + LIMIT/OFFSET added around it left it
-- unbounded because the cost is per-row, not per-result-page.
--
-- Plain (non-CONCURRENTLY) CREATE INDEX, matching this repo's established
-- convention for a table sized like this one (e.g.
-- 20260713000001_operator_turn_chunks_text_trgm.sql, 47.7k rows): sqlx runs
-- each migration in its own transaction and CONCURRENTLY cannot run inside
-- one, migrations apply at startup before the proxy binds its ports (no
-- concurrent writers to block), and a btree build over 172k bigint rows
-- completes in well under a second.

CREATE INDEX IF NOT EXISTS idx_rsp_firing_rules_request_system_prompt_id
    ON requests.system_prompt_firing_rules (request_system_prompt_id);
