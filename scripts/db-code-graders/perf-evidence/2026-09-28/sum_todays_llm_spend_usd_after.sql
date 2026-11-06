SELECT COALESCE(SUM(cost_usd), 0.0)::float8 AS "total!"
  FROM llm.daily_spend
 WHERE day = (now() AT TIME ZONE 'utc')::date
