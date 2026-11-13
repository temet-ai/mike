SELECT s.at AS "at!", q.subject AS "subject!", q.detail AS "detail!", q.fires AS "fires!"
  FROM unnest($4::timestamptz[]) AS s(at)
 CROSS JOIN LATERAL (
    WITH c AS (
        SELECT role, outcome, cost_usd, created_at FROM llm.calls
         WHERE created_at >= s.at - interval '24 hours' AND created_at < s.at
    ),
    agg AS (
        SELECT role, count(*) AS attempts,
               count(*) FILTER (WHERE outcome = 'ok') AS ok,
               count(*) FILTER (WHERE outcome = 'refused_ceiling') AS refused_ceiling,
               count(*) FILTER (WHERE outcome = 'refused_rate') AS refused_rate,
               count(*) FILTER (WHERE outcome = 'refused_spend') AS refused_spend,
               count(*) FILTER (WHERE outcome IN ('refused_ceiling', 'refused_rate', 'refused_spend')) AS refused,
               coalesce(sum(cost_usd) FILTER (WHERE created_at >= date_trunc('day', s.at)), 0) AS spend_today
          FROM c GROUP BY role
    ),
    cfg AS (
        SELECT coalesce(jsonb_object_agg(key, value), '{}'::jsonb) AS settings
          FROM config.settings WHERE key = ANY($3::text[])
    )
    SELECT 'role:' || agg.role AS subject,
           jsonb_build_object('attempts', agg.attempts, 'ok', agg.ok,
                              'refused_ceiling', agg.refused_ceiling, 'refused_rate', agg.refused_rate,
                              'refused_spend', agg.refused_spend, 'refused', agg.refused,
                              'spend_today_usd', agg.spend_today, 'settings', cfg.settings) AS detail,
           agg.refused >= $1::bigint AND agg.refused >= $2::float8 * agg.attempts AS fires
      FROM agg CROSS JOIN cfg
     ORDER BY agg.role
 ) q
 ORDER BY s.at, q.subject
