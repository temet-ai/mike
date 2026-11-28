WITH RECURSIVE stats AS (
    SELECT s.queryid,
           sum(s.calls) AS calls,
           sum(s.rows) AS rows,
           min(s.stats_since) AS stats_since,
           (array_agg(s.query ORDER BY s.calls DESC))[1] AS query
      FROM pg_stat_statements s
     WHERE s.dbid = (SELECT oid FROM pg_database WHERE datname = current_database())
       AND s.toplevel
       AND s.queryid IS NOT NULL
       AND s.userid IN (SELECT r.oid FROM pg_roles r
                         WHERE r.rolname IN (SELECT jsonb_array_elements_text('{"roles":["devuser"],"skip_text":"tenders|awards","calls_per_hour":60,"reads_per_write":20,"min_window_minutes":30}'::jsonb->'roles')))
       AND s.query !~* '\m(insert\s+into|update\s+[a-z_."]+(\s+(as\s+)?[a-z_]+)?\s+set|delete\s+from|merge\s+into|copy)\M'
       AND s.query !~* ('{"roles":["devuser"],"skip_text":"tenders|awards","calls_per_hour":60,"reads_per_write":20,"min_window_minutes":30}'::jsonb->>'skip_text')
     GROUP BY s.queryid
),
base AS (
    SELECT DISTINCT ON (c.queryid) c.*
      FROM wright.statement_counters c
     WHERE c.taken_at <= now() - make_interval(mins => ('{"roles":["devuser"],"skip_text":"tenders|awards","calls_per_hour":60,"reads_per_write":20,"min_window_minutes":30}'::jsonb->>'min_window_minutes')::int)
       AND EXISTS (SELECT 1 FROM wright.table_counters t WHERE t.taken_at = c.taken_at)
     ORDER BY c.queryid, c.taken_at DESC
),
busy AS MATERIALIZED (
    SELECT st.queryid, st.query, b.taken_at AS span_at,
           st.calls - b.calls AS span_calls,
           st.rows - b.rows AS span_rows,
           extract(epoch FROM now() - b.taken_at) / 3600 AS span_hours
      FROM stats st
      JOIN base b ON b.queryid = st.queryid
                 AND b.stats_since IS NOT DISTINCT FROM st.stats_since
                 AND b.calls <= st.calls
     WHERE (st.calls - b.calls) / (extract(epoch FROM now() - b.taken_at) / 3600)
           >= ('{"roles":["devuser"],"skip_text":"tenders|awards","calls_per_hour":60,"reads_per_write":20,"min_window_minutes":30}'::jsonb->>'calls_per_hour')::numeric
),
edges AS (
    SELECT DISTINCT rw.ev_class AS parent, d.refobjid AS child
      FROM pg_rewrite rw
      JOIN pg_depend d ON d.classid = 'pg_rewrite'::regclass AND d.objid = rw.oid
                      AND d.refclassid = 'pg_class'::regclass AND d.refobjid <> rw.ev_class
    UNION
    SELECT i.inhparent, i.inhrelid FROM pg_inherits i
),
reads (queryid, relid) AS (
    SELECT b.queryid, r.relid FROM busy b CROSS JOIN LATERAL (
        SELECT DISTINCT c.oid AS relid
          FROM regexp_matches(b.query,
                   '\m(?:from|join)\s+(?:only\s+)?(?:"?([a-z_][a-z0-9_$]*)"?\.)?"?([a-z_][a-z0-9_$]*)',
                   'gi') AS m
          JOIN pg_namespace n ON n.nspname = lower(coalesce(m[1], 'public'))
          JOIN pg_class c ON c.relnamespace = n.oid AND c.relname = lower(m[2])
         WHERE c.relkind IN ('r', 'p', 'v', 'm')
           AND n.nspname <> 'information_schema'
           AND n.nspname !~ '^pg_'
    ) r
    UNION
    SELECT rd.queryid, e.child FROM reads rd JOIN edges e ON e.parent = rd.relid
),
writes AS (
    SELECT rd.queryid,
           t.schemaname || '.' || t.relname AS name,
           t.n_tup_ins + t.n_tup_upd + t.n_tup_del - coalesce(tc.writes, 0) AS span_writes,
           b.span_hours
      FROM reads rd
      JOIN busy b ON b.queryid = rd.queryid
      JOIN pg_stat_user_tables t ON t.relid = rd.relid
      LEFT JOIN wright.table_counters tc ON tc.relid = t.relid AND tc.taken_at = b.span_at
),
judged AS (
    SELECT w.queryid,
           sum(w.span_writes) AS span_writes,
           bool_or(w.span_writes < 0) AS reset,
           jsonb_agg(jsonb_build_object(
               'table', w.name,
               'window_writes', w.span_writes,
               'writes_per_hour', round(w.span_writes / w.span_hours, 1))
             ORDER BY w.span_writes DESC, w.name) AS tables
      FROM writes w
     GROUP BY w.queryid
)
SELECT 'query'::text AS scope,
       'query ' || b.queryid || ': '
           || left(regexp_replace(btrim(b.query), '\s+', ' ', 'g'), 120) AS resource,
       jsonb_build_object(
           'queryid', b.queryid::text,
           'calls_per_hour', round(b.span_calls / b.span_hours, 1),
           'writes_per_hour', round(j.span_writes / b.span_hours, 1),
           'reads_per_write', round(b.span_calls / greatest(j.span_writes, 1), 1),
           'tables', j.tables,
           'window_rows_per_call', round(b.span_rows / nullif(b.span_calls, 0), 2),
           'window_since', b.span_at,
           'window_hours', round(b.span_hours, 2),
           'window_calls', b.span_calls,
           'window_writes', j.span_writes,
           'query', regexp_replace(btrim(b.query), '\s+', ' ', 'g'),
           'thresholds', '{"roles":["devuser"],"skip_text":"tenders|awards","calls_per_hour":60,"reads_per_write":20,"min_window_minutes":30}'::jsonb
       ) AS evidence
  FROM busy b
  JOIN judged j ON j.queryid = b.queryid
 WHERE NOT j.reset
   AND b.span_calls / greatest(j.span_writes, 1) >= ('{"roles":["devuser"],"skip_text":"tenders|awards","calls_per_hour":60,"reads_per_write":20,"min_window_minutes":30}'::jsonb->>'reads_per_write')::numeric
 ORDER BY b.span_calls / greatest(j.span_writes, 1) DESC, b.queryid
 LIMIT 200;
