SELECT
    fr.id,
    MAX(recent.created_at) AS last_fired_at,
    COUNT(*) FILTER (WHERE recent.created_at >= NOW() - INTERVAL '1 day')::BIGINT AS fires_24h,
    COUNT(recent.created_at)::BIGINT AS fires_7d
FROM rules.rules fr
LEFT JOIN (
    SELECT j.filter_rule_id, r.created_at
    FROM requests.requests r
    JOIN requests.system_prompts rsp ON rsp.request_id = r.id
    JOIN requests.system_prompt_firing_rules j ON j.request_system_prompt_id = rsp.id
    WHERE r.created_at >= NOW() - INTERVAL '7 days'
) recent ON recent.filter_rule_id = fr.id
GROUP BY fr.id
