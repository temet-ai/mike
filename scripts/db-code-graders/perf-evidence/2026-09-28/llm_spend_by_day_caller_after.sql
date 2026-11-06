WITH agg AS (
     SELECT day, role, steerer_id,
            COALESCE(SUM(cost_usd), 0.0)::float8 AS cost_usd,
            COALESCE(SUM(call_count), 0)::bigint AS call_count,
            COALESCE(SUM(prompt_tokens), 0)::bigint AS prompt_tokens,
            COALESCE(SUM(completion_tokens), 0)::bigint AS completion_tokens,
            COALESCE(SUM(reasoning_tokens), 0)::bigint AS reasoning_tokens,
            COALESCE(SUM(failed_count), 0)::bigint AS failed_count
       FROM llm.daily_spend
      WHERE 1 = 1
        AND day >= '2026-08-31'::date
        AND day <= '2026-09-28'::date
      GROUP BY day, role, steerer_id
     )
     SELECT to_char(a.day, 'YYYY-MM-DD') AS day, a.role AS role,
            a.steerer_id AS steerer_id, st.name AS steerer_name,
            st.display_name AS steerer_display_name,
            a.cost_usd AS cost_usd, a.call_count AS call_count,
            a.prompt_tokens AS prompt_tokens, a.completion_tokens AS completion_tokens,
            a.reasoning_tokens AS reasoning_tokens, a.failed_count AS failed_count
       FROM agg a
       LEFT JOIN steering.steerers st ON st.id = a.steerer_id
      ORDER BY a.day, a.role, a.steerer_id
