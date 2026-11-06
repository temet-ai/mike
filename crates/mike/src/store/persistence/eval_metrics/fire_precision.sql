WITH fires AS (
    SELECT i.id, i.steerer_id, st.kind AS class, i.confidence
      FROM steering.interventions i
      JOIN steering.steerers st ON st.id = i.steerer_id
      JOIN steering.verdicts v ON v.id = i.verdict_id AND v.code = 'fired'
     WHERE i.created_at >= $1::timestamptz - interval '30 days' AND i.created_at < $1::timestamptz
),
lab AS (
    SELECT l.subject_id, l.label, l.tier, l.split
      FROM eval.labels_as_of($3) l
     WHERE l.subject_kind = 'intervention' AND l.axis = 'steer_valid'
       AND l.label IN ('right', 'wrong') AND l.tier IN ('gold', 'relayed')
),
scored AS (
    SELECT f.steerer_id, f.class, lab.tier, lab.split, lab.label = 'right' AS ok,
           CASE WHEN f.confidence < 0.5 THEN '[0,0.5)' WHEN f.confidence < 0.7 THEN '[0.5,0.7)'
                WHEN f.confidence < 0.85 THEN '[0.7,0.85)' ELSE '[0.85,1]' END AS bin,
           to_char(least(floor(coalesce(f.confidence, 0) * 20), 19) * 0.05, 'FM0.00') AS bin05
      FROM fires f JOIN lab ON lab.subject_id = f.id
),
agg AS (
    SELECT steerer_id, class, tier, split, NULL::text AS dimension, NULL::text AS dimension_value,
           count(*) FILTER (WHERE ok) AS k, count(*) AS n
      FROM scored GROUP BY steerer_id, class, tier, split
    UNION ALL
    SELECT steerer_id, class, tier, split, 'confidence_bin', bin, count(*) FILTER (WHERE ok), count(*)
      FROM scored GROUP BY steerer_id, class, tier, split, bin
    UNION ALL
    SELECT steerer_id, class, tier, split, 'confidence_bin_05', bin05, count(*) FILTER (WHERE ok), count(*)
      FROM scored GROUP BY steerer_id, class, tier, split, bin05
)
INSERT INTO eval.rates (rollup_id, family, metric, population, scope_kind, window_start,
                        window_end, window_days, role, axis, class, label_tier, label_split,
                        dimension, dimension_value, steerer_id, question, mode, k, n, value,
                        ci_low, ci_high, weighted, detail)
SELECT $2, 'steer_calibration', 'fire_precision', 'steerer_evaluations', 'global',
       $1::timestamptz - interval '30 days', $1::timestamptz, 30, 'steerer', 'steer_valid', agg.class, agg.tier, agg.split,
       agg.dimension, agg.dimension_value, agg.steerer_id, agg.class, 'live', agg.k::int,
       agg.n::int, agg.k::float8 / nullif(agg.n, 0),
       eval.wilson_low(agg.k::float8 / nullif(agg.n, 0), agg.n),
       eval.wilson_high(agg.k::float8 / nullif(agg.n, 0), agg.n), false, '{}'::jsonb
  FROM agg
