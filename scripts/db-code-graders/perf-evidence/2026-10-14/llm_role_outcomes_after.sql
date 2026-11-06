WITH recent AS (
     SELECT role, model, outcome, http_status, created_at,
            row_number() OVER (PARTITION BY role ORDER BY created_at DESC, id DESC) AS rn
       FROM llm.calls
      WHERE created_at > now() - interval '7 days'
        AND role <> 'eval'
   )
   SELECT role,
          (array_agg(model ORDER BY rn))[1] AS model,
          (array_agg(outcome ORDER BY rn))[1] AS last_outcome,
          (array_agg(http_status ORDER BY rn))[1] AS last_http_status,
          max(created_at) AS last_call_at,
          max(created_at) FILTER (WHERE outcome = 'ok') AS last_ok_at,
          COALESCE(min(rn) FILTER (WHERE outcome = 'ok') - 1, count(*))::bigint AS failed_streak
     FROM recent
    GROUP BY role
    ORDER BY role
