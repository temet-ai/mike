SELECT COALESCE(SUM(cost_usd), 0.0)::float8 AS "total!"
  FROM llm.calls
 WHERE created_at >= date_trunc('day', now() AT TIME ZONE 'utc')
