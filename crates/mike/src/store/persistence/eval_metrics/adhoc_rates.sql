SELECT r.id, r.rollup_id, ro.for_day, r.family, r.metric, r.population, r.scope_kind, r.project_id,
       r.window_start, r.window_end, r.window_days, r.role, r.bundle_id, r.bundle_label, r.axis,
       r.class, r.label_tier, r.label_split, r.label_source, r.dimension, r.dimension_value,
       r.steerer_id, r.question, r.mode, r.k, r.n, r.value, r.ci_low, r.ci_high, r.weighted,
       r.n_effective, r.detail
  FROM eval.rates r
  JOIN eval.rollups ro ON ro.id = r.rollup_id
 WHERE r.rollup_id = $1
 ORDER BY r.family, r.role, r.axis, r.class, r.label_tier, r.label_split, r.label_source,
          r.bundle_label NULLS FIRST, r.window_days NULLS FIRST, r.dimension_value, r.id
