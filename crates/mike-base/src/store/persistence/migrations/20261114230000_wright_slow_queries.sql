-- slow_queries: Wright names each slow or full-scan statement from pg_stat_statements, one
-- problem per queryid, instead of one log_warnings problem for every sqlx slow-statement line.
-- pg_stat_statements is cumulative, so a query stays flagged only while its call count keeps
-- rising past the open episode's last evidence; quiet_hours without a call heals it.
-- omit got this row live through POST /api/wright/checks; ON CONFLICT keeps that row.
INSERT INTO wright.checks (name, source, cadence, expects, probe, query, params) VALUES
('slow_queries', 'capture_db', 'regular',
 'No query that ran in the last quiet_hours averages more than mean_ms or reads more than blocks_per_call shared blocks per call; each slow or full-scan query is named by its pg_stat_statements queryid',
 'sql',
 $q$WITH stats AS (
    SELECT s.queryid,
           sum(s.calls) AS calls,
           sum(s.total_exec_time) AS total_ms,
           max(s.max_exec_time) AS max_ms,
           sum(s.rows) AS rows,
           sum(s.shared_blks_hit + s.shared_blks_read) AS blocks,
           sum(s.shared_blks_read) AS blocks_read,
           min(s.stats_since) AS stats_since,
           (array_agg(s.query ORDER BY s.calls DESC))[1] AS query
      FROM pg_stat_statements s
     WHERE s.dbid = (SELECT oid FROM pg_database WHERE datname = current_database())
       AND s.toplevel
       AND s.queryid IS NOT NULL
       AND s.query !~* '^\s*explain'
     GROUP BY s.queryid
),
heavy AS (
    SELECT st.*,
           st.total_ms / st.calls AS mean_ms,
           st.blocks / st.calls AS blocks_per_call,
           'query ' || st.queryid || ': '
               || left(regexp_replace(btrim(st.query), '\s+', ' ', 'g'), 120) AS resource
      FROM stats st
     WHERE st.calls >= ($1->>'min_calls')::bigint
       AND (st.total_ms / st.calls >= ($1->>'mean_ms')::numeric
            OR st.blocks / st.calls >= ($1->>'blocks_per_call')::numeric)
),
prior AS (
    SELECT DISTINCT ON (p.resource) p.resource, e.evidence
      FROM wright.problems p
      JOIN wright.episodes e ON e.problem_id = p.id AND e.closed_at IS NULL
     WHERE p.check_name = 'slow_queries' AND p.state = 'open'
     ORDER BY p.resource, e.id DESC
),
judged AS (
    SELECT h.*,
           CASE
               WHEN pr.evidence IS NULL
                 OR h.calls > (pr.evidence->>'calls')::bigint
                 OR (pr.evidence->>'calls')::bigint > h.calls THEN now()
               ELSE (pr.evidence->>'last_called_at')::timestamptz
           END AS last_called_at
      FROM heavy h
      LEFT JOIN prior pr ON pr.resource = h.resource
)
SELECT 'query'::text AS scope,
       j.resource,
       jsonb_build_object(
           'queryid', j.queryid::text,
           'calls', j.calls,
           'last_called_at', j.last_called_at,
           'mean_ms', round(j.mean_ms::numeric, 1),
           'max_ms', round(j.max_ms::numeric, 1),
           'total_seconds', round((j.total_ms / 1000)::numeric, 1),
           'rows_per_call', round(j.rows::numeric / j.calls, 1),
           'blocks_per_call', round(j.blocks_per_call::numeric),
           'mb_read_per_call', round(j.blocks_per_call::numeric * 8 / 1024, 1),
           'disk_blocks_read', j.blocks_read,
           'counted_since', j.stats_since,
           'query', left(j.query, 4000),
           'thresholds', $1
       ) AS evidence
  FROM judged j
 WHERE j.last_called_at >= now() - make_interval(hours => ($1->>'quiet_hours')::int)
 ORDER BY j.total_ms DESC
 LIMIT 200$q$,
 '{"min_calls": 20, "mean_ms": 500, "blocks_per_call": 100000, "quiet_hours": 24}')
ON CONFLICT (name) DO NOTHING;
