-- reads_outpace_writes: a lookup should run about as often as its data changes, not as often as
-- its caller wakes up. idle_polls sees only statements that return almost no rows, so a statement
-- that returns rows every call was invisible however wasteful it was. On 2026-11-23 a hand
-- measurement found the withheld-lessons read (about 171 rows a call, 600 calls an hour), the
-- agent alias map (about 141 rows a call, 600 an hour) and the remote-control channel lookup
-- (1,468 an hour) re-reading tables that are rarely written; Wright had flagged none of them.
--
-- The check compares each read-only statement's call rate with the write rate of the tables it
-- reads (n_tup_ins + n_tup_upd + n_tup_del in pg_stat_user_tables), over the same window as
-- idle_polls. It finds the tables a statement reads from its FROM and JOIN clauses, follows views
-- to their tables and partitioned tables to their partitions, and reports a statement run
-- calls_per_hour or more times an hour that is read reads_per_write or more times per row written
-- to its tables. A statement with no table it can name is not judged.
--
-- wright.table_counters holds the tables' write counters at each taken_at of
-- wright.statement_counters: the runner writes them in the same transaction as the statement
-- snapshot, before idle_polls or reads_outpace_writes runs, and deletes a table snapshot once no
-- statement snapshot has its taken_at.

CREATE TABLE wright.table_counters (
    taken_at timestamptz NOT NULL,
    relid    oid         NOT NULL,
    writes   numeric     NOT NULL,
    PRIMARY KEY (taken_at, relid)
);

INSERT INTO wright.checks (name, source, cadence, expects, probe, query, params, enabled)
SELECT 'reads_outpace_writes', 'capture_db', 'regular',
       $expects$No read-only mike statement runs calls_per_hour or more times an hour while it is read reads_per_write or more times per row written to the tables it reads (inserts, updates and deletes in pg_stat_user_tables). Each statement is judged over the idle_polls window, from its newest snapshot in wright.statement_counters at least min_window_minutes old and the table counters saved with it in wright.table_counters. Tables come from its FROM and JOIN clauses, through views and partitions; a statement with no table it names is not judged.$expects$,
       'sql',
       $q$WITH RECURSIVE stats AS (
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
                         WHERE r.rolname IN (SELECT jsonb_array_elements_text($1->'roles')))
       AND s.query !~* '\m(insert\s+into|update\s+[a-z_."]+(\s+(as\s+)?[a-z_]+)?\s+set|delete\s+from|merge\s+into|copy)\M'
       AND s.query !~* ($1->>'skip_text')
     GROUP BY s.queryid
),
base AS (
    SELECT DISTINCT ON (c.queryid) c.*
      FROM wright.statement_counters c
     WHERE c.taken_at <= now() - make_interval(mins => ($1->>'min_window_minutes')::int)
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
           >= ($1->>'calls_per_hour')::numeric
),
rels AS MATERIALIZED (
    SELECT c.oid AS relid,
           '\m(from|join)\s+(only\s+)?'
               || CASE WHEN n.nspname = 'public' THEN '("?public"?\.)?'
                       ELSE '"?' || n.nspname || '"?\.' END
               || '"?' || c.relname || '\M' AS pattern
      FROM pg_class c
      JOIN pg_namespace n ON n.oid = c.relnamespace
     WHERE c.relkind IN ('r', 'p', 'v', 'm')
       AND n.nspname <> 'information_schema'
       AND n.nspname !~ '^pg_'
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
    SELECT b.queryid, r.relid FROM busy b JOIN rels r ON b.query ~* r.pattern
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
           'query', left(regexp_replace(btrim(b.query), '\s+', ' ', 'g'), 160),
           'thresholds', $1
       ) AS evidence
  FROM busy b
  JOIN judged j ON j.queryid = b.queryid
 WHERE NOT j.reset
   AND b.span_calls / greatest(j.span_writes, 1) >= ($1->>'reads_per_write')::numeric
 ORDER BY b.span_calls / greatest(j.span_writes, 1) DESC, b.queryid
 LIMIT 200$q$,
       jsonb_build_object(
           'roles', coalesce((SELECT c.params->'roles' FROM wright.checks c WHERE c.name = 'idle_polls'),
                             '["devuser"]'::jsonb),
           'skip_text', coalesce((SELECT c.params->'skip_text' FROM wright.checks c WHERE c.name = 'idle_polls'),
                                 '"tenders|awards"'::jsonb),
           'calls_per_hour', 60,
           'reads_per_write', 20,
           'min_window_minutes', 30),
       true
ON CONFLICT (name) DO NOTHING;
