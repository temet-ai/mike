WITH ev AS (
    SELECT e.steerer_id, coalesce(e.question, st.kind) AS class, e.question, e.mode, e.status,
           s.project_id
      FROM steering.evaluations e
      JOIN steering.steerers st ON st.id = e.steerer_id
      JOIN sessions.sessions s ON s.id = e.session_id
     WHERE e.dispatched_at >= $1::timestamptz - interval '30 days' AND e.dispatched_at < $1::timestamptz
       AND e.status <> 'dispatched'
),
agg AS (
    SELECT steerer_id, class, question, mode, NULL::bigint AS project_id,
           count(*) FILTER (WHERE status = 'resolved_correction') AS k, count(*) AS n
      FROM ev GROUP BY steerer_id, class, question, mode
    UNION ALL
    SELECT steerer_id, class, question, mode, project_id,
           count(*) FILTER (WHERE status = 'resolved_correction'), count(*)
      FROM ev WHERE project_id IS NOT NULL GROUP BY steerer_id, class, question, mode, project_id
)
INSERT INTO eval.rates (rollup_id, family, metric, population, scope_kind, project_id,
                        window_start, window_end, window_days, role, axis, class, steerer_id,
                        question, mode, k, n, value, ci_low, ci_high, weighted, detail)
SELECT $2, 'steer_calibration', 'fire_rate', 'steerer_evaluations',
       CASE WHEN agg.project_id IS NULL THEN 'global' ELSE 'project' END, agg.project_id,
       $1::timestamptz - interval '30 days', $1::timestamptz, 30, 'steerer', 'steer_valid', agg.class, agg.steerer_id,
       agg.question, agg.mode, agg.k::int, agg.n::int, agg.k::float8 / nullif(agg.n, 0),
       eval.wilson_low(agg.k::float8 / nullif(agg.n, 0), agg.n),
       eval.wilson_high(agg.k::float8 / nullif(agg.n, 0), agg.n), false, '{}'::jsonb
  FROM agg
