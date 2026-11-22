-- idle_polls could not see a statement it had never flagged: with no episode
-- to hold a base, the check judged it over pg_stat_statements' whole history,
-- since stats_since, and that diluted the rate. On 2026-11-22 the open-questions
-- read ran 1822 times an hour with no rows, and five Wright and fleet reads ran
-- 60 to 88 times an hour; none was reported.
--
-- The watch runner now saves the counters of every candidate statement before
-- idle_polls runs, in wright.statement_counters. A statement gets a new
-- snapshot once its newest is min_window_minutes old, and at most two are kept,
-- so the table holds about two rows per statement and does not grow. idle_polls
-- judges each statement from the newest snapshot that is at least
-- min_window_minutes old, which spans 30 to about 65 minutes. A statement with
-- no such snapshot, or whose counters were reset since, is not judged.
-- Episodes no longer carry window bases, so the prior, based and window-roll
-- logic goes, with the window_hours param.

CREATE TABLE wright.statement_counters (
    queryid     bigint           NOT NULL,
    taken_at    timestamptz      NOT NULL,
    calls       numeric          NOT NULL,
    rows        numeric          NOT NULL,
    total_ms    double precision NOT NULL,
    stats_since timestamptz,
    PRIMARY KEY (queryid, taken_at)
);

UPDATE wright.checks
   SET query = $idle_polls$WITH stats AS (
    SELECT s.queryid,
           sum(s.calls) AS calls,
           sum(s.rows) AS rows,
           sum(s.total_exec_time) AS total_ms,
           min(s.stats_since) AS stats_since,
           (array_agg(s.query ORDER BY s.calls DESC))[1] AS query
      FROM pg_stat_statements s
     WHERE s.dbid = (SELECT oid FROM pg_database WHERE datname = current_database())
       AND s.toplevel
       AND s.queryid IS NOT NULL
       AND s.userid IN (SELECT r.oid FROM pg_roles r
                         WHERE r.rolname IN (SELECT jsonb_array_elements_text($1->'roles')))
       AND s.query !~* '^\s*(begin|commit|rollback|end|start\s+transaction|savepoint|release|set|reset|show|listen|unlisten|notify|discard|deallocate|prepare|explain|vacuum|analyze)\M'
       AND s.query !~* '^\s*select\s+(1|[$]1)\s*;?\s*$'
       AND s.query !~* ($1->>'skip_text')
     GROUP BY s.queryid
),
base AS (
    SELECT DISTINCT ON (c.queryid) c.*
      FROM wright.statement_counters c
     WHERE c.taken_at <= now() - make_interval(mins => ($1->>'min_window_minutes')::int)
     ORDER BY c.queryid, c.taken_at DESC
),
spans AS (
    SELECT st.*,
           b.taken_at AS span_at,
           st.calls - b.calls AS span_calls,
           st.rows - b.rows AS span_rows,
           st.total_ms - b.total_ms AS span_ms,
           extract(epoch FROM now() - b.taken_at) / 3600 AS span_hours
      FROM stats st
      JOIN base b ON b.queryid = st.queryid
                 AND b.stats_since IS NOT DISTINCT FROM st.stats_since
                 AND b.calls <= st.calls
),
rated AS (
    SELECT sp.*,
           sp.span_calls / sp.span_hours AS calls_per_hour,
           sp.span_rows / nullif(sp.span_calls, 0) AS rows_per_call
      FROM spans sp
)
SELECT 'query'::text AS scope,
       'query ' || r.queryid || ': '
           || left(regexp_replace(btrim(r.query), '\s+', ' ', 'g'), 120) AS resource,
       jsonb_build_object(
           'queryid', r.queryid::text,
           'calls_per_hour', round(r.calls_per_hour, 1),
           'window_rows_per_call', round(r.rows_per_call, 5),
           'window_since', r.span_at,
           'window_hours', round(r.span_hours, 2),
           'window_calls', r.span_calls,
           'window_rows', r.span_rows,
           'window_seconds', round((r.span_ms / 1000)::numeric, 1),
           'calls', r.calls,
           'rows', r.rows,
           'total_ms', round(r.total_ms::numeric, 1),
           'counted_since', r.stats_since,
           'query', left(regexp_replace(btrim(r.query), '\s+', ' ', 'g'), 160),
           'thresholds', $1
       ) AS evidence
  FROM rated r
 WHERE r.calls_per_hour >= ($1->>'calls_per_hour')::numeric
   AND r.rows_per_call < ($1->>'max_rows_per_call')::numeric
 ORDER BY r.calls_per_hour DESC, r.queryid
 LIMIT 200$idle_polls$,
       params = params - 'window_hours',
       expects = $expects$No mike statement runs calls_per_hour or more times an hour with fewer than max_rows_per_call rows a call. Before each pass the runner saves every candidate statement's counters in wright.statement_counters, a new snapshot once the newest is min_window_minutes old, two kept. Each statement is judged from the newest snapshot at least min_window_minutes old, over 30 to about 65 minutes. A statement with no such snapshot, or whose counters were reset since it, is not judged.$expects$,
       updated_at = now()
 WHERE name = 'idle_polls';
