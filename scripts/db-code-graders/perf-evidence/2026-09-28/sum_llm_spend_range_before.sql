SELECT COALESCE(SUM(cost_usd), 0.0)::float8
  FROM llm.calls
 WHERE created_at >= '2026-08-31T00:00:00Z'::timestamptz
   AND ('2026-09-28T23:59:59Z'::timestamptz IS NULL OR created_at < '2026-09-28T23:59:59Z'::timestamptz)
