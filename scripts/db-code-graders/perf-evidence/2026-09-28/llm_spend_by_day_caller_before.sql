WITH agg AS (
     SELECT date_trunc('day', c.created_at AT TIME ZONE 'utc') AS day_ts,
            c.role,
            c.steerer_id,
            COALESCE(SUM(c.cost_usd), 0.0)::float8 AS cost_usd,
            COUNT(*)::bigint AS call_count,
            COALESCE(SUM(c.prompt_tokens), 0)::bigint AS prompt_tokens,
            COALESCE(SUM(c.completion_tokens), 0)::bigint AS completion_tokens,
            COALESCE(SUM(c.reasoning_tokens), 0)::bigint AS reasoning_tokens,
            COUNT(*) FILTER (WHERE c.outcome NOT IN ('ok', 'truncated'))::bigint AS failed_count
       FROM llm.calls c
      WHERE 1 = 1
        AND c.created_at >= '2026-08-31T00:00:00Z'::timestamptz
        AND c.created_at < '2026-09-28T23:59:59Z'::timestamptz
      GROUP BY 1, 2, 3
     )
     SELECT to_char(a.day_ts, 'YYYY-MM-DD') AS day, a.role AS role,
            a.steerer_id AS steerer_id, st.name AS steerer_name,
            st.display_name AS steerer_display_name,
            a.cost_usd AS cost_usd, a.call_count AS call_count,
            a.prompt_tokens AS prompt_tokens, a.completion_tokens AS completion_tokens,
            a.reasoning_tokens AS reasoning_tokens, a.failed_count AS failed_count
       FROM agg a
       LEFT JOIN steering.steerers st ON st.id = a.steerer_id
      ORDER BY a.day_ts, a.role, a.steerer_id
