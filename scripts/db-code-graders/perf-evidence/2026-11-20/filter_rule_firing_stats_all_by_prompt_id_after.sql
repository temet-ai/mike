WITH bounds AS MATERIALIZED (
    SELECT (SELECT rsp.id FROM requests.system_prompts rsp
             WHERE rsp.request_id >= (SELECT r.id FROM requests.requests r
                                       WHERE r.created_at >= NOW() - INTERVAL '7 days'
                                       ORDER BY r.created_at LIMIT 1)
             ORDER BY rsp.request_id LIMIT 1) AS week_prompt_id,
           (SELECT rsp.id FROM requests.system_prompts rsp
             WHERE rsp.request_id >= (SELECT r.id FROM requests.requests r
                                       WHERE r.created_at >= NOW() - INTERVAL '1 day'
                                       ORDER BY r.created_at LIMIT 1)
             ORDER BY rsp.request_id LIMIT 1) AS day_prompt_id
), per_rule AS (
    SELECT j.filter_rule_id,
           MAX(j.request_system_prompt_id) AS last_prompt_id,
           COUNT(*) FILTER (WHERE j.request_system_prompt_id >= (SELECT b.day_prompt_id FROM bounds b)) AS fires_24h,
           COUNT(*) AS fires_7d
    FROM requests.system_prompt_firing_rules j
    WHERE j.request_system_prompt_id >= (SELECT b.week_prompt_id FROM bounds b)
    GROUP BY j.filter_rule_id
)
SELECT
    fr.id AS "id!",
    lr.created_at AS last_fired_at,
    COALESCE(pr.fires_24h, 0)::BIGINT AS "fires_24h!",
    COALESCE(pr.fires_7d, 0)::BIGINT AS "fires_7d!"
FROM rules.rules fr
LEFT JOIN per_rule pr ON pr.filter_rule_id = fr.id
LEFT JOIN requests.system_prompts lp ON lp.id = pr.last_prompt_id
LEFT JOIN requests.requests lr ON lr.id = lp.request_id
