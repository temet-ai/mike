INSERT INTO eval.rates (rollup_id, family, metric, population, scope_kind, window_start,
                        window_end, window_days, role, axis, class, dimension, dimension_value,
                        k, n, value, ci_low, ci_high, weighted, detail)
SELECT $1, 'action_effect', 'share', x.population, 'global',
       $2::timestamptz - make_interval(days => $3::int), $2::timestamptz, $3,
       'steerer', 'reaction', 'corrected', 'guard_question', x.question,
       x.k, x.n, x.value, x.ci_low, x.ci_high, false, '{}'::jsonb
  FROM unnest($4::text[], $5::text[], $6::int[], $7::int[], $8::float8[], $9::float8[], $10::float8[])
       AS x(population, question, k, n, value, ci_low, ci_high)
