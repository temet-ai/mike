WITH stats AS (
    SELECT s.queryid,
           sum(s.calls) AS calls,
           sum(s.rows) AS rows,
           sum(s.total_exec_time) AS total_ms,
           min(s.stats_since) AS stats_since
      FROM pg_stat_statements s
     WHERE s.dbid = (SELECT oid FROM pg_database WHERE datname = current_database())
       AND s.toplevel
       AND s.queryid IS NOT NULL
       AND s.userid IN (SELECT r.oid FROM pg_roles r
                         WHERE r.rolname IN (SELECT jsonb_array_elements_text($1->'roles')))
     GROUP BY s.queryid
),
newest AS (
    SELECT DISTINCT ON (c.queryid) c.queryid, c.taken_at, c.calls, c.stats_since
      FROM wright.statement_counters c
     ORDER BY c.queryid, c.taken_at DESC
)
INSERT INTO wright.statement_counters (queryid, taken_at, calls, rows, total_ms, stats_since)
SELECT st.queryid, now(), st.calls, st.rows, st.total_ms, st.stats_since
  FROM stats st
  LEFT JOIN newest n ON n.queryid = st.queryid
 WHERE n.queryid IS NULL
    OR n.taken_at <= now() - make_interval(mins => ($1->>'min_window_minutes')::int)
    OR n.stats_since IS DISTINCT FROM st.stats_since
    OR n.calls > st.calls
ON CONFLICT (queryid, taken_at) DO NOTHING
