SELECT role AS key,
       COUNT(*)::bigint AS call_count,
       COALESCE(SUM(cost_usd), 0.0)::float8 AS cost_usd,
       COALESCE(SUM(prompt_tokens), 0)::bigint AS prompt_tokens,
       COALESCE(SUM(completion_tokens), 0)::bigint AS completion_tokens,
       COALESCE(SUM(reasoning_tokens), 0)::bigint AS reasoning_tokens
  FROM llm.calls
 WHERE 1 = 1
   AND created_at >= '2026-08-31T00:00:00Z'::timestamptz
   AND created_at < '2026-09-28T23:59:59Z'::timestamptz
 GROUP BY role
 ORDER BY role
