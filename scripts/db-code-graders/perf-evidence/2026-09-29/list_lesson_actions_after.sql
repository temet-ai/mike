WITH latest AS (
    SELECT DISTINCT ON (dimension_value, population)
           dimension_value, population, k, n, window_start, window_end
      FROM eval.rates
     WHERE family = 'action_effect' AND dimension = 'lesson'
     ORDER BY dimension_value, population, window_end DESC, id DESC
)
SELECT l.id AS lesson_id, l.text, ls.code AS stage, lsrc.code AS source, l.created_at,
       p.project_path,
       bef.k AS k_before, bef.n AS n_before,
       bef.window_start AS window_start_before, bef.window_end AS window_end_before,
       aft.k AS k_after, aft.n AS n_after,
       aft.window_start AS window_start_after, aft.window_end AS window_end_after
  FROM memory.lessons l
  JOIN memory.lesson_stages ls ON ls.id = l.stage_id
  JOIN memory.lesson_sources lsrc ON lsrc.id = l.source_id
  LEFT JOIN projects.projects p ON p.id = l.project_id
  LEFT JOIN latest bef ON bef.dimension_value = l.id::text AND bef.population = 'before'
  LEFT JOIN latest aft ON aft.dimension_value = l.id::text AND aft.population = 'after'
 WHERE lsrc.code IN ('judge', 'directive')
   AND ls.code IN ('daily', 'long_term')
   AND (NULL::text IS NULL OR p.project_path = NULL::text)
 ORDER BY l.created_at DESC;
