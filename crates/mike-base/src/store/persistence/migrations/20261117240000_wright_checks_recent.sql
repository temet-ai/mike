-- Two Wright checks reported faults that were not there.
--
-- decisions_health flagged a decision point "silent today" whenever it made any decision the day
-- before and none today. Most points run only when their event happens (a fleet, a merge, a
-- blocked dialog), so on 2026-11-17 one daily run opened 16 problems for points that had run a
-- handful of times. A point is now silent only when it made at least min_silent_calls decisions
-- the day before, so a steady point that stops is still caught.
--
-- slow_queries judged each statement by its pg_stat_statements average since the counters were
-- last reset, so a query made fast stayed flagged for as long as its old calls dominated the
-- average. It now judges the calls since a base it keeps in the episode evidence (base_calls,
-- base_total_ms, base_blocks, base_at): the window covers at least quiet_hours and moves on once
-- it has min_calls calls. Until a window has min_calls calls the last window's verdict stands, and
-- a query never seen before is judged on its whole history as before. The latest episode of the
-- problem is read whether it is open or closed, so a healed query is not reopened from its stale
-- all-time average.
UPDATE wright.checks
   SET params = params || '{"min_silent_calls": 20}'::jsonb,
       query = replace(query,
                       $old$    SELECT DISTINCT point
      FROM decisions.records
     WHERE created_at >= now() - make_interval(hours => 2 * ($1->>'window_hours')::int)
       AND created_at < now() - make_interval(hours => ($1->>'window_hours')::int)
       AND mode IN ('shadow', 'on')
),$old$,
                       $new$    SELECT point
      FROM decisions.records
     WHERE created_at >= now() - make_interval(hours => 2 * ($1->>'window_hours')::int)
       AND created_at < now() - make_interval(hours => ($1->>'window_hours')::int)
       AND mode IN ('shadow', 'on')
     GROUP BY point
    HAVING count(*) >= COALESCE(($1->>'min_silent_calls')::int, 20)
),$new$),
       expects = 'Every decision point that ran in the last day has calls that mostly succeed: failures under the share in params, no point that made at least min_silent_calls decisions yesterday silent today while its steerer still fires, and no Jev call refused by the decisions daily spend ceiling',
       updated_at = now()
 WHERE name = 'decisions_health';

UPDATE wright.checks
   SET expects = 'No query averages more than mean_ms or reads more than blocks_per_call shared blocks per call over its recent calls (at least min_calls since a base kept in the evidence, a window of at least quiet_hours), and it ran in the last quiet_hours; each is named by its pg_stat_statements queryid',
       query = $q$WITH stats AS (
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
prior AS (
    SELECT p.resource, e.evidence AS pe, e.closed_at IS NULL AS open
      FROM wright.problems p
     CROSS JOIN LATERAL (
         SELECT ep.evidence, ep.closed_at
           FROM wright.episodes ep
          WHERE ep.problem_id = p.id
          ORDER BY ep.id DESC
          LIMIT 1
     ) e
     WHERE p.check_name = 'slow_queries'
),
named AS (
    SELECT st.*,
           'query ' || st.queryid || ': '
               || left(regexp_replace(btrim(st.query), '\s+', ' ', 'g'), 120) AS resource
      FROM stats st
      LEFT JOIN (SELECT DISTINCT pr.pe->>'queryid' AS queryid FROM prior pr) known
        ON known.queryid = st.queryid::text
     WHERE st.calls >= ($1->>'min_calls')::bigint
       AND (st.total_ms / st.calls >= ($1->>'mean_ms')::numeric
            OR st.blocks / st.calls >= ($1->>'blocks_per_call')::numeric
            OR known.queryid IS NOT NULL)
),
based AS (
    SELECT n.*, pr.pe, COALESCE(pr.open, false) AS open,
           (pr.pe IS NULL OR (pr.pe->>'calls')::numeric > n.calls) AS fresh,
           pr.pe->>'base_at' IS NULL AS unbased
      FROM named n
      LEFT JOIN prior pr USING (resource)
),
base AS (
    SELECT b.*,
           CASE WHEN b.fresh THEN 0
                WHEN b.unbased THEN (b.pe->>'calls')::numeric
                ELSE (b.pe->>'base_calls')::numeric END AS base_calls,
           CASE WHEN b.fresh THEN 0
                WHEN b.unbased THEN (b.pe->>'total_seconds')::numeric * 1000
                ELSE (b.pe->>'base_total_ms')::numeric END AS base_total_ms,
           CASE WHEN b.fresh THEN 0
                WHEN b.unbased THEN (b.pe->>'blocks_per_call')::numeric * (b.pe->>'calls')::numeric
                ELSE (b.pe->>'base_blocks')::numeric END AS base_blocks,
           CASE WHEN b.fresh THEN b.stats_since
                WHEN b.unbased THEN now()
                ELSE (b.pe->>'base_at')::timestamptz END AS base_at
      FROM based b
),
windowed AS (
    SELECT w.*,
           w.calls - w.base_calls AS window_calls,
           CASE WHEN w.calls - w.base_calls >= ($1->>'min_calls')::numeric
                THEN (w.total_ms - w.base_total_ms) / (w.calls - w.base_calls)
                ELSE COALESCE((w.pe->>'window_mean_ms')::numeric, w.total_ms / w.calls)
           END AS window_mean_ms,
           CASE WHEN w.calls - w.base_calls >= ($1->>'min_calls')::numeric
                THEN (w.blocks - w.base_blocks) / (w.calls - w.base_calls)
                ELSE COALESCE((w.pe->>'window_blocks_per_call')::numeric, w.blocks / w.calls)
           END AS window_blocks_per_call,
           CASE WHEN w.pe IS NULL
                  OR w.calls > (w.pe->>'calls')::numeric
                  OR (w.pe->>'calls')::numeric > w.calls THEN now()
                ELSE COALESCE((w.pe->>'last_called_at')::timestamptz, now())
           END AS last_called_at,
           (w.base_at < now() - make_interval(hours => ($1->>'quiet_hours')::int)
            AND w.calls - w.base_calls >= ($1->>'min_calls')::numeric) AS rolls
      FROM base w
)
SELECT 'query'::text AS scope,
       j.resource,
       jsonb_build_object(
           'queryid', j.queryid::text,
           'calls', j.calls,
           'last_called_at', j.last_called_at,
           'mean_ms', round(j.window_mean_ms::numeric, 1),
           'window_calls', j.window_calls,
           'window_since', j.base_at,
           'window_mean_ms', round(j.window_mean_ms::numeric, 1),
           'window_blocks_per_call', round(j.window_blocks_per_call::numeric),
           'mean_ms_since_counted', round((j.total_ms / j.calls)::numeric, 1),
           'base_calls', CASE WHEN j.rolls THEN j.calls ELSE j.base_calls END,
           'base_total_ms', round((CASE WHEN j.rolls THEN j.total_ms ELSE j.base_total_ms END)::numeric, 1),
           'base_blocks', CASE WHEN j.rolls THEN j.blocks ELSE j.base_blocks END,
           'base_at', CASE WHEN j.rolls THEN now() ELSE j.base_at END,
           'max_ms', round(j.max_ms::numeric, 1),
           'total_seconds', round((j.total_ms / 1000)::numeric, 1),
           'rows_per_call', round(j.rows::numeric / j.calls, 1),
           'blocks_per_call', round(j.window_blocks_per_call::numeric),
           'mb_read_per_call', round(j.window_blocks_per_call::numeric * 8 / 1024, 1),
           'disk_blocks_read', j.blocks_read,
           'counted_since', j.stats_since,
           'query', left(j.query, 4000),
           'thresholds', $1
       ) AS evidence
  FROM windowed j
 WHERE (j.window_mean_ms >= ($1->>'mean_ms')::numeric
        OR j.window_blocks_per_call >= ($1->>'blocks_per_call')::numeric)
   AND j.last_called_at >= now() - make_interval(hours => ($1->>'quiet_hours')::int)
 ORDER BY j.total_ms DESC
 LIMIT 200$q$,
       updated_at = now()
 WHERE name = 'slow_queries';
