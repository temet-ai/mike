SELECT role AS key,
       COALESCE(SUM(call_count), 0)::bigint AS call_count,
       COALESCE(SUM(cost_usd), 0.0)::float8 AS cost_usd,
       COALESCE(SUM(prompt_tokens), 0)::bigint AS prompt_tokens,
       COALESCE(SUM(completion_tokens), 0)::bigint AS completion_tokens,
       COALESCE(SUM(reasoning_tokens), 0)::bigint AS reasoning_tokens
  FROM llm.daily_spend
 WHERE 1 = 1
   AND day >= '2026-08-31'::date
   AND day <= '2026-09-28'::date
 GROUP BY role
 ORDER BY role
