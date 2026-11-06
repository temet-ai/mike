INSERT INTO eval.rates (rollup_id, family, metric, population, scope_kind, project_id,
                        window_start, window_end, window_days, role, axis, class, label_tier,
                        label_split, label_source, dimension, dimension_value, k, n, value,
                        ci_low, ci_high, weighted, detail)
SELECT $4, 'base_rate', 'share', q.population,
       CASE WHEN q.project_id IS NULL THEN 'global' ELSE 'project' END, q.project_id,
       $2 - make_interval(days => w.days), $2, w.days,
       CASE WHEN q.axis = 'session_end' THEN 'session_judge' ELSE 'turn_judge' END,
       q.axis, q.class, q.label_tier, q.label_split,
       CASE WHEN q.label_source = 'labels' THEN NULL ELSE q.label_source END,
       CASE WHEN q.axis = 'session_end' THEN NULL ELSE 'ended_on' END,
       CASE WHEN q.axis = 'session_end' THEN NULL ELSE q.dimension_value END,
       q.k::int, q.n::int, q.k::float8 / nullif(q.n, 0),
       eval.wilson_low(q.k::float8 / nullif(q.n, 0), q.n),
       eval.wilson_high(q.k::float8 / nullif(q.n, 0), q.n), false,
       jsonb_build_object('source', q.label_source)
  FROM unnest($3::int[]) AS w(days)
  CROSS JOIN LATERAL (
    WITH pop AS (
        SELECT p.operator_turn_id, p.project_id, p.is_replied, p.ended_on, p.next_state, p.legacy_outcome, p.bundle_label
          FROM eval.turn_population_v p
         WHERE p.is_real AND p.created_at < $2 AND p.created_at >= $2 - make_interval(days => w.days)
    ),
    ranked AS (
        SELECT l.operator_turn_id, l.axis, l.label, l.tier,
               row_number() OVER (PARTITION BY l.operator_turn_id, l.axis
                                  ORDER BY CASE l.tier WHEN 'gold' THEN 1 WHEN 'relayed' THEN 2
                                                       WHEN 'seed' THEN 3 ELSE 4 END,
                                           l.created_at DESC, l.id DESC) AS rk
          FROM eval.labels_as_of($1) l
         WHERE l.split = 'dev'
           AND ((l.subject_kind = 'turn' AND l.axis = 'reaction')
                OR (l.subject_kind = 'session_end' AND l.axis = 'session_end'))
    ),
    best AS (
        SELECT operator_turn_id, axis, label, tier FROM ranked WHERE rk = 1),
    lab_rows AS (
        SELECT b.tier, coalesce(p.ended_on, 'unknown') AS ended_on, p.project_id,
               b.label = 'corrected' AS pos
          FROM pop p JOIN best b ON b.operator_turn_id = p.operator_turn_id AND b.axis = 'reaction'
         WHERE p.is_replied
    ),
    frus_rows AS (
        SELECT b.tier, p.project_id, b.label = 'closed_in_frustration' AS pos
          FROM pop p JOIN best b ON b.operator_turn_id = p.operator_turn_id AND b.axis = 'session_end'
         WHERE p.next_state = 'none_session_settled'
    ),
    prod_rows AS (
        SELECT coalesce(p.ended_on, 'unknown') AS ended_on, p.project_id,
               p.legacy_outcome = 'corrected' AS pos, p.bundle_label
          FROM pop p
         WHERE p.is_replied
    ),
    parts AS (
        SELECT 'reaction' AS axis, 'corrected' AS class, 'replied_real_turns' AS population,
               tier AS label_tier, 'dev' AS label_split, 'labels' AS label_source,
               CASE WHEN GROUPING(ended_on) = 1 THEN 'all' ELSE ended_on END AS dimension_value,
               NULL::bigint AS project_id, count(*) FILTER (WHERE pos) AS k, count(*) AS n
          FROM lab_rows GROUP BY GROUPING SETS ((tier, ended_on), (tier))
        UNION ALL
        SELECT 'reaction', 'corrected', 'replied_real_turns', tier, 'dev', 'labels', 'all', project_id,
               count(*) FILTER (WHERE pos), count(*)
          FROM lab_rows WHERE project_id IS NOT NULL GROUP BY tier, project_id HAVING count(*) >= 30
        UNION ALL
        SELECT 'legacy_outcome', 'corrected', 'replied_real_turns', NULL, NULL, 'production',
               CASE WHEN GROUPING(ended_on) = 1 THEN 'all' ELSE ended_on END, NULL::bigint,
               count(*) FILTER (WHERE pos), count(*)
          FROM prod_rows GROUP BY GROUPING SETS ((ended_on), ())
        UNION ALL
        SELECT 'legacy_outcome', 'corrected', 'replied_real_turns', NULL, NULL, 'production', 'all',
               project_id, count(*) FILTER (WHERE pos), count(*)
          FROM prod_rows WHERE project_id IS NOT NULL GROUP BY project_id HAVING count(*) >= 30
        UNION ALL
        SELECT 'session_end', 'closed_in_frustration', 'real_session_ends', tier, 'dev', 'labels',
               'all', NULL::bigint, count(*) FILTER (WHERE pos), count(*)
          FROM frus_rows GROUP BY tier
        UNION ALL
        SELECT 'session_end', 'closed_in_frustration', 'real_session_ends', tier, 'dev', 'labels',
               'all', project_id, count(*) FILTER (WHERE pos), count(*)
          FROM frus_rows WHERE project_id IS NOT NULL GROUP BY tier, project_id HAVING count(*) >= 30
    )
    SELECT population, project_id, axis, class, label_tier, label_split, label_source,
           dimension_value, k, n
      FROM parts
  ) q
