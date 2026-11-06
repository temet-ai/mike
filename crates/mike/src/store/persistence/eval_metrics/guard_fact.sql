WITH pop AS (
    SELECT p.operator_turn_id, p.project_id
      FROM eval.turn_population_v p
     WHERE p.is_real AND p.is_replied AND p.created_at < $1 AND p.created_at >= $1 - interval '30 days'
),
ranked AS (
    SELECT l.operator_turn_id, l.label,
           row_number() OVER (PARTITION BY l.operator_turn_id
                              ORDER BY CASE l.tier WHEN 'gold' THEN 1 WHEN 'relayed' THEN 2
                                                   WHEN 'seed' THEN 3 ELSE 4 END,
                                       l.created_at DESC, l.id DESC) AS rk
      FROM eval.labels_as_of($3) l
     WHERE l.split = 'dev' AND l.subject_kind = 'turn' AND l.axis = 'reaction'
),
best AS (
    SELECT operator_turn_id, label FROM ranked WHERE rk = 1
),
prev AS (
    SELECT t.id AS turn_id, lag(t.id) OVER (PARTITION BY t.session_id ORDER BY t.created_at, t.id) AS prev_id
      FROM memory.operator_turns t
     WHERE t.created_at < $1 AND t.created_at >= $1 - interval '31 days'
),
after_corrected AS (
    SELECT DISTINCT pr.turn_id
      FROM prev pr
      JOIN memory.turn_verdicts v ON v.operator_turn_id = pr.prev_id AND v.mode = 'live' AND v.reaction = 'corrected'
),
fact_rows AS (
    SELECT 'after_corrected_turn' AS fact, p.project_id, b.label = 'corrected' AS pos
      FROM pop p
      JOIN after_corrected ac ON ac.turn_id = p.operator_turn_id
      JOIN best b ON b.operator_turn_id = p.operator_turn_id
    UNION ALL
    SELECT 'unobserved_after_change', p.project_id, b.label = 'corrected'
      FROM pop p
      JOIN memory.turn_facts tf ON tf.operator_turn_id = p.operator_turn_id AND tf.observed_after_change = false
      JOIN best b ON b.operator_turn_id = p.operator_turn_id
),
parts AS (
    SELECT fact, NULL::bigint AS project_id, count(*) FILTER (WHERE pos) AS k, count(*) AS n
      FROM fact_rows GROUP BY fact
    UNION ALL
    SELECT fact, project_id, count(*) FILTER (WHERE pos), count(*)
      FROM fact_rows WHERE project_id IS NOT NULL GROUP BY fact, project_id
)
INSERT INTO eval.rates (rollup_id, family, metric, population, scope_kind, project_id, window_start,
                        window_end, window_days, role, axis, class, label_split, dimension, dimension_value,
                        k, n, value, ci_low, ci_high, weighted, detail)
SELECT $2, 'base_rate', 'share', 'replied_real_turns',
       CASE WHEN p.project_id IS NULL THEN 'global' ELSE 'project' END, p.project_id,
       $1 - interval '30 days', $1, 30, 'turn_judge', 'reaction', 'corrected', 'dev', 'guard_fact', p.fact,
       p.k::int, p.n::int, p.k::float8 / nullif(p.n, 0),
       eval.wilson_low(p.k::float8 / nullif(p.n, 0), p.n),
       eval.wilson_high(p.k::float8 / nullif(p.n, 0), p.n), false,
       jsonb_build_object('source', 'labels')
  FROM parts p
