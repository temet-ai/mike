SELECT COALESCE(SUM(cost_usd), 0.0)::float8
  FROM llm.daily_spend
 WHERE day >= '2026-08-31'::date
   AND ('2026-09-28'::date IS NULL OR day <= '2026-09-28'::date)
