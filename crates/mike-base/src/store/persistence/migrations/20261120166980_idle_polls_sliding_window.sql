-- idle_polls reopened healed problems from stale windows and from windows only
-- seconds old. At 19:14 UTC on 2026-11-22 six statements running at 0-6 calls an
-- hour were reported: four (problems 69644, 69653, 69659, 69660) on the window that
-- began at 13:12 UTC, before their fixes, because a healed problem kept its old
-- base and the pass that rolled it judged the whole stale window; two (69640,
-- 69645) on a window 1-4 calls old that carried the previous window's rate.
--
-- The check now judges each statement over at least min_window_minutes of recent
-- calls, never fewer, and never carries a rate:
--   * an open problem keeps two bases, base (the current window's start) and
--     prev_base (the previous window's start). It is judged over [base, now] once
--     that is min_window_minutes old, and over [prev_base, now] before then;
--   * a window rolls after window_hours: prev_base takes base, base takes now,
--     and that pass judges the closing window [base, now];
--   * a healed problem is judged from the counters of the last pass that flagged
--     it (observed_at, or the episode's last_seen_at for older evidence), so the
--     calls from before it healed never count again;
--   * a statement with no span of min_window_minutes is not reported, so it heals.
-- The rule that needed a call since the last evidence goes: no rate is carried
-- any more, and the rule made a burst-and-pause poller open and heal every pass.

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
prior AS (
    SELECT DISTINCT ON (e.evidence->>'queryid')
           e.evidence->>'queryid' AS queryid,
           e.evidence AS pe,
           e.closed_at IS NULL AS open,
           COALESCE((e.evidence->>'observed_at')::timestamptz, e.last_seen_at) AS seen_at
      FROM wright.problems p
      JOIN wright.episodes e ON e.problem_id = p.id
     WHERE p.check_name = 'idle_polls'
     ORDER BY e.evidence->>'queryid', e.id DESC
),
based AS (
    SELECT st.*, pr.pe, pr.seen_at,
           CASE WHEN pr.pe IS NULL
                  OR (pr.pe->>'calls')::numeric > st.calls
                  OR (pr.pe->>'counted_since')::timestamptz IS DISTINCT FROM st.stats_since
                THEN 'fresh'
                WHEN pr.open THEN 'open'
                ELSE 'closed' END AS kind
      FROM stats st
      LEFT JOIN prior pr ON pr.queryid = st.queryid::text
),
points AS (
    SELECT b.*,
           CASE b.kind WHEN 'fresh' THEN b.stats_since
                       WHEN 'open' THEN (b.pe->>'base_at')::timestamptz
                       ELSE b.seen_at END AS cur_at,
           CASE b.kind WHEN 'fresh' THEN 0
                       WHEN 'open' THEN (b.pe->>'base_calls')::numeric
                       ELSE (b.pe->>'calls')::numeric END AS cur_calls,
           CASE b.kind WHEN 'fresh' THEN 0
                       WHEN 'open' THEN (b.pe->>'base_rows')::numeric
                       ELSE (b.pe->>'rows')::numeric END AS cur_rows,
           CASE b.kind WHEN 'fresh' THEN 0
                       WHEN 'open' THEN (b.pe->>'base_total_ms')::numeric
                       ELSE COALESCE((b.pe->>'total_ms')::numeric,
                                     (b.pe->>'total_seconds')::numeric * 1000) END AS cur_total_ms,
           CASE WHEN b.kind = 'open' THEN (b.pe->>'prev_base_at')::timestamptz END AS prev_at,
           CASE WHEN b.kind = 'open' THEN (b.pe->>'prev_base_calls')::numeric END AS prev_calls,
           CASE WHEN b.kind = 'open' THEN (b.pe->>'prev_base_rows')::numeric END AS prev_rows,
           CASE WHEN b.kind = 'open' THEN (b.pe->>'prev_base_total_ms')::numeric END AS prev_total_ms
      FROM based b
),
spans AS (
    SELECT p.*,
           make_interval(mins => ($1->>'min_window_minutes')::int) AS min_span,
           now() - p.cur_at >= make_interval(mins => ($1->>'min_window_minutes')::int) AS cur_ready,
           now() - p.cur_at >= make_interval(hours => ($1->>'window_hours')::int) AS rolls
      FROM points p
),
judged AS (
    SELECT s.*,
           CASE WHEN s.cur_ready THEN 'this window'
                WHEN now() - s.prev_at >= s.min_span THEN 'this and the previous window' END AS judged_on,
           CASE WHEN s.cur_ready THEN s.cur_at ELSE s.prev_at END AS span_at,
           s.calls - CASE WHEN s.cur_ready THEN s.cur_calls ELSE s.prev_calls END AS span_calls,
           s.rows - CASE WHEN s.cur_ready THEN s.cur_rows ELSE s.prev_rows END AS span_rows,
           s.total_ms - CASE WHEN s.cur_ready THEN s.cur_total_ms ELSE s.prev_total_ms END AS span_ms
      FROM spans s
),
rated AS (
    SELECT j.*,
           extract(epoch FROM now() - j.span_at) / 3600 AS span_hours,
           j.span_calls / (extract(epoch FROM now() - j.span_at) / 3600) AS calls_per_hour,
           j.span_rows / nullif(j.span_calls, 0) AS rows_per_call
      FROM judged j
     WHERE j.judged_on IS NOT NULL
)
SELECT 'query'::text AS scope,
       'query ' || r.queryid || ': '
           || left(regexp_replace(btrim(r.query), '\s+', ' ', 'g'), 120) AS resource,
       jsonb_build_object(
           'queryid', r.queryid::text,
           'calls_per_hour', round(r.calls_per_hour, 1),
           'window_rows_per_call', round(r.rows_per_call, 5),
           'judged_on', r.judged_on,
           'window_since', r.span_at,
           'window_hours', round(r.span_hours, 2),
           'window_calls', r.span_calls,
           'window_rows', r.span_rows,
           'window_seconds', round((r.span_ms / 1000)::numeric, 1),
           'observed_at', now(),
           'calls', r.calls,
           'rows', r.rows,
           'total_ms', round(r.total_ms::numeric, 1),
           'total_seconds', round((r.total_ms / 1000)::numeric, 1),
           'counted_since', r.stats_since,
           'base_at', CASE WHEN r.rolls THEN now() ELSE r.cur_at END,
           'base_calls', CASE WHEN r.rolls THEN r.calls ELSE r.cur_calls END,
           'base_rows', CASE WHEN r.rolls THEN r.rows ELSE r.cur_rows END,
           'base_total_ms', round((CASE WHEN r.rolls THEN r.total_ms ELSE r.cur_total_ms END)::numeric, 1),
           'prev_base_at', CASE WHEN r.rolls THEN r.cur_at ELSE r.prev_at END,
           'prev_base_calls', CASE WHEN r.rolls THEN r.cur_calls ELSE r.prev_calls END,
           'prev_base_rows', CASE WHEN r.rolls THEN r.cur_rows ELSE r.prev_rows END,
           'prev_base_total_ms', round((CASE WHEN r.rolls THEN r.cur_total_ms ELSE r.prev_total_ms END)::numeric, 1),
           'query', left(regexp_replace(btrim(r.query), '\s+', ' ', 'g'), 160),
           'thresholds', $1
       ) AS evidence
  FROM rated r
 WHERE r.calls_per_hour >= ($1->>'calls_per_hour')::numeric
   AND r.rows_per_call < ($1->>'max_rows_per_call')::numeric
 ORDER BY r.calls_per_hour DESC, r.queryid
 LIMIT 200$idle_polls$,
       params = params || '{"window_hours": 1, "min_window_minutes": 30}'::jsonb,
       expects = $expects$No mike statement runs calls_per_hour or more times an hour with fewer than max_rows_per_call rows a call. Each statement is judged over at least min_window_minutes of recent calls: its current window once that is old enough, otherwise the current and previous windows together. A window rolls after window_hours. After a problem heals, its statement is judged from the last pass that flagged it. A statement with no span that long is not reported.$expects$,
       updated_at = now()
 WHERE name = 'idle_polls';
