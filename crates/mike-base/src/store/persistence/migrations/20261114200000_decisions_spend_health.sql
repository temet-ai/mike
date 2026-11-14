-- decisions_health also flags the day the decisions spend ceiling
-- (decisions.daily_spend_ceiling_usd, default 5 USD) refused Jev calls, with the
-- spend so far today and the ceiling in force (#221 item 4).
UPDATE wright.checks
   SET expects = 'Every decision point that ran in the last day has calls that mostly succeed: failures under the share in params, no point that ran yesterday silent today while its steerer still fires, and no Jev call refused by the decisions daily spend ceiling',
       query = $q$WITH today AS (
    SELECT point,
           count(*) AS calls,
           count(*) FILTER (WHERE outcome <> 'ok') AS failures,
           count(*) FILTER (WHERE outcome = 'schema_error') AS schema_errors,
           count(*) FILTER (WHERE outcome LIKE 'refused_%' OR outcome = 'no_key') AS refused,
           sum(question_refusals) AS question_refusals,
           count(*) FILTER (WHERE mode = 'shadow') AS shadow,
           COALESCE(sum(cost_usd), 0) AS cost_usd
      FROM decisions.records
     WHERE created_at >= now() - make_interval(hours => ($1->>'window_hours')::int)
       AND mode IN ('shadow', 'on')
     GROUP BY point
),
before AS (
    SELECT DISTINCT point
      FROM decisions.records
     WHERE created_at >= now() - make_interval(hours => 2 * ($1->>'window_hours')::int)
       AND created_at < now() - make_interval(hours => ($1->>'window_hours')::int)
       AND mode IN ('shadow', 'on')
),
fires AS (
    SELECT count(*) AS n
      FROM steering.evaluations e
      JOIN steering.steerers s ON s.id = e.steerer_id
     WHERE s.name = 'turn-guard'
       AND e.dispatched_at >= now() - make_interval(hours => ($1->>'window_hours')::int)
       AND e.status = 'resolved_correction'
),
ceiling AS (
    SELECT count(*) FILTER (WHERE r.outcome = 'refused_spend') AS refused,
           COALESCE(sum(r.cost_usd), 0) AS spent_usd,
           count(DISTINCT r.point) FILTER (WHERE r.outcome = 'refused_spend') AS points
      FROM decisions.records r
     WHERE r.created_at >= date_trunc('day', now() AT TIME ZONE 'utc') AT TIME ZONE 'utc'
)
SELECT 'decisions'::text AS scope,
       t.point AS resource,
       jsonb_build_object('calls', t.calls, 'failures', t.failures,
                          'schema_errors', t.schema_errors, 'refused', t.refused,
                          'question_refusals', t.question_refusals, 'shadow', t.shadow,
                          'cost_usd', round(t.cost_usd::numeric, 6)) AS evidence
  FROM today t
 WHERE t.failures::float8 / t.calls > ($1->>'max_failure_share')::float8
UNION ALL
SELECT 'decisions'::text, b.point,
       jsonb_build_object('calls', 0, 'live_fires_today', f.n,
                          'note', 'this point made decisions yesterday and none today')
  FROM before b CROSS JOIN fires f
 WHERE NOT EXISTS (SELECT 1 FROM today t WHERE t.point = b.point) AND f.n > 0
UNION ALL
SELECT 'decisions'::text, 'daily_spend_ceiling',
       jsonb_build_object('refused_spend', c.refused, 'points_refused', c.points,
                          'spent_today_usd', round(c.spent_usd::numeric, 6),
                          'ceiling_usd', COALESCE((SELECT s.value FROM config.settings s
                                                    WHERE s.key = 'decisions.daily_spend_ceiling_usd'),
                                                  '5'::jsonb),
                          'note', 'the decisions daily spend ceiling was reached; every point took its off path')
  FROM ceiling c
 WHERE c.refused > 0
 LIMIT 200$q$
 WHERE name = 'decisions_health';
