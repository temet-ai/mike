WITH promotions AS (
    SELECT (cl.subject_key)::bigint AS lesson_id, min(cl.changed_at) AS promoted_at
      FROM config.change_log cl
     WHERE cl.subject_kind = 'lesson_decision' AND cl.new_value ->> 'stage' = 'daily'
     GROUP BY cl.subject_key
),
targets AS (
    SELECT l.id AS lesson_id, l.project_id, lsrc.code AS source_code,
           COALESCE(pr.promoted_at, l.created_at) AS anchor_at
      FROM memory.lessons l
      JOIN memory.lesson_sources lsrc ON lsrc.id = l.source_id
      JOIN memory.lesson_stages ls ON ls.id = l.stage_id
      LEFT JOIN promotions pr ON pr.lesson_id = l.id
     WHERE lsrc.code IN ('judge', 'directive')
       AND ls.code IN ('daily', 'long_term')
       AND l.project_id IS NOT NULL
),
ranked AS (
    SELECT lb.operator_turn_id, lb.label, lb.tier, lb.created_at,
           row_number() OVER (PARTITION BY lb.operator_turn_id
                              ORDER BY CASE lb.tier WHEN 'gold' THEN 1 WHEN 'relayed' THEN 2
                                                   WHEN 'seed' THEN 3 ELSE 4 END,
                                       lb.created_at DESC, lb.id DESC) AS rk
      FROM eval.labels_as_of('2026-10-04T23:00:00Z'::timestamptz) lb
     WHERE lb.split = 'dev' AND lb.subject_kind = 'turn' AND lb.axis = 'reaction'
),
best AS (SELECT operator_turn_id, label FROM ranked WHERE rk = 1),
scored AS (
    SELECT t.lesson_id, t.project_id, t.anchor_at, p.created_at,
           (b.label = 'corrected') AS is_corrected
      FROM targets t
      JOIN eval.turn_population_v p ON p.project_id = t.project_id
      JOIN best b ON b.operator_turn_id = p.operator_turn_id
     WHERE p.is_real AND p.is_replied
       AND p.created_at >= t.anchor_at - make_interval(days => 14)
       AND p.created_at < t.anchor_at + make_interval(days => 14)
),
buckets AS (
    SELECT lesson_id, project_id, anchor_at,
           CASE WHEN created_at < anchor_at THEN 'before' ELSE 'after' END AS population,
           count(*) FILTER (WHERE is_corrected) AS k, count(*) AS n
      FROM scored
     GROUP BY lesson_id, project_id, anchor_at, (created_at < anchor_at)
)
SELECT 1, 'action_effect', 'share', b.population, 'project', b.project_id,
       CASE WHEN b.population = 'before' THEN b.anchor_at - make_interval(days => 14)
            ELSE b.anchor_at END,
       CASE WHEN b.population = 'before' THEN b.anchor_at
            ELSE b.anchor_at + make_interval(days => 14) END,
       14, 'turn_judge', 'reaction', 'corrected', 'lesson', b.lesson_id::text,
       b.k::int, b.n::int, b.k::float8 / nullif(b.n, 0),
       eval.wilson_low(b.k::float8 / nullif(b.n, 0), b.n),
       eval.wilson_high(b.k::float8 / nullif(b.n, 0), b.n),
       false, jsonb_build_object('anchor_at', b.anchor_at)
  FROM buckets b