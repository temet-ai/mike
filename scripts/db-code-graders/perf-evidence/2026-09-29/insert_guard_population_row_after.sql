INSERT INTO eval.rates (rollup_id, family, metric, population, scope_kind,
                         window_start, window_end, window_days, role, axis, class,
                         dimension, dimension_value, k, n, value, ci_low, ci_high,
                         weighted, detail)
 VALUES (1, 'action_effect', 'share', 'holdback', 'global',
         '2026-09-29 00:00:00+00'::timestamptz - make_interval(days => 14::int),
         '2026-09-29 00:00:00+00'::timestamptz, 14,
         'steerer', 'reaction', 'corrected',
         'guard_question', 'should_ask_clarifying',
         3, 34, 0.0882, 0.0303, 0.2261,
         false, '{}'::jsonb);
