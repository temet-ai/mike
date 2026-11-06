SELECT model AS key,
       COUNT(*)::bigint AS call_count,
       COALESCE(SUM(cost_usd), 0.0)::float8 AS cost_usd,
       COALESCE(SUM(prompt_tokens), 0)::bigint AS prompt_tokens,
       COALESCE(SUM(completion_tokens), 0)::bigint AS completion_tokens,
       COALESCE(SUM(reasoning_tokens), 0)::bigint AS reasoning_tokens
  FROM llm.calls
 WHERE created_at >= '2026-08-01T00:00:00Z'
 GROUP BY model
 ORDER BY model;
