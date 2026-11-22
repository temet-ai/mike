-- Two Wright checks report a problem only while it still happens: a steerer's
-- wasted calls only if the last one was within recent_minutes, a failing engineer
-- streak only if its newest failure was. Before, a fixed problem stayed open for the
-- whole 24 h lookback.

UPDATE wright.checks SET query = $q$WITH verifier AS (
    SELECT bool_or(st.enabled AND st.deleted_at IS NULL) AS any_enabled,
           max(COALESCE(st.deleted_at, st.updated_at)) AS last_off_at
      FROM steering.steerers st
     WHERE st.kind = 'verifier'
),
calls AS (
    SELECT c.id, c.created_at, c.latency_ms, c.steerer_id,
           COALESCE(st.name, 'steerer ' || c.steerer_id || ' (row gone)') AS steerer_name, st.kind,
           COALESCE(st.deleted_at, st.updated_at) AS disabled_at
      FROM llm.calls c
      LEFT JOIN steering.steerers st ON st.id = c.steerer_id
     WHERE c.role = 'steerer'
       AND c.created_at >= now() - make_interval(hours => ($1->>'hours')::int)
       AND c.steerer_id IS NOT NULL
       AND NOT COALESCE(st.enabled AND st.deleted_at IS NULL, false)
       AND c.created_at >= COALESCE(st.deleted_at, st.updated_at, '-infinity'::timestamptz)
    UNION ALL
    SELECT c.id, c.created_at, c.latency_ms, NULL, 'criteria writer', 'verifier', v.last_off_at
      FROM llm.calls c
     CROSS JOIN verifier v
     WHERE c.role = 'steerer'
       AND c.created_at >= now() - make_interval(hours => ($1->>'hours')::int)
       AND c.steerer_id IS NULL
       AND NOT COALESCE(v.any_enabled, false)
       AND c.created_at >= COALESCE(v.last_off_at, '-infinity'::timestamptz)
       AND COALESCE(c.request_body->>'system', c.request_body->'messages'->0->>'content')
           LIKE 'You write acceptance criteria%'
)
SELECT 'steerer'::text AS scope,
       c.steerer_name AS resource,
       jsonb_build_object(
           'steerer_id', min(c.steerer_id),
           'kind', min(c.kind),
           'disabled_at', min(c.disabled_at),
           'window_hours', ($1->>'hours')::int,
           'calls', count(*),
           'latency_seconds', round(sum(COALESCE(c.latency_ms, 0)) / 1000.0, 1),
           'first_call', min(c.created_at),
           'last_call', max(c.created_at),
           'last_call_id', max(c.id)
       ) AS evidence
  FROM calls c
 GROUP BY c.steerer_name
HAVING max(c.created_at) >= now() - make_interval(mins => ($1->>'recent_minutes')::int)
 ORDER BY count(*) DESC, c.steerer_name
 LIMIT 200$q$,
       params = params || '{"recent_minutes": 60}'::jsonb
 WHERE name = 'steering_work_without_reader';

UPDATE wright.checks SET query = $q$
WITH recent AS (
    SELECT id, mode, state, problem_id, end_reason, ended_at,
           row_number() OVER (ORDER BY id DESC) AS place
      FROM wright.investigations
     WHERE ended_at IS NOT NULL
       AND state <> 'unavailable'
       AND ended_at >= now() - make_interval(hours => ($1->>'window_hours')::int)
),
failing AS (
    SELECT * FROM recent
     WHERE place <= ($1->>'in_a_row')::int AND state IN ('failed', 'killed')
)
SELECT 'wright_engineer'::text AS scope, 'runs_ending_without_a_result'::text AS resource,
       jsonb_build_object(
           'in_a_row', count(*),
           'runs', jsonb_agg(jsonb_build_object('id', id, 'mode', mode, 'state', state,
                   'problem_id', problem_id, 'ended_at', ended_at, 'end_reason', left(end_reason, 500))
                   ORDER BY id DESC)) AS evidence
  FROM failing
HAVING count(*) >= ($1->>'in_a_row')::int
   AND max(ended_at) >= now() - make_interval(mins => ($1->>'recent_minutes')::int)
 LIMIT 200$q$,
       params = params || '{"recent_minutes": 180}'::jsonb
 WHERE name = 'wright_engineer_runs';
