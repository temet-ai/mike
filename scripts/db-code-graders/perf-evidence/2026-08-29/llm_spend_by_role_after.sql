SELECT role AS key,
       COUNT(*)::bigint AS call_count,
       COALESCE(SUM(cost_usd), 0.0)::float8 AS cost_usd,
       COALESCE(SUM(prompt_tokens), 0)::bigint AS prompt_tokens,
       COALESCE(SUM(completion_tokens), 0)::bigint AS completion_tokens,
       COALESCE(SUM(reasoning_tokens), 0)::bigint AS reasoning_tokens
  FROM llm.calls
 GROUP BY role
 ORDER BY role;
