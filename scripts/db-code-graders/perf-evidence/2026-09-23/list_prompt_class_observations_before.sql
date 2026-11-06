SELECT sp.request_id, sp.session_id, sp.created_at, sp.content_sha256,
       sp.altered, r.model,
       COUNT(*) OVER ()::bigint AS "total_count!"
FROM system_prompts.session_prompts sp
LEFT JOIN requests.requests r ON r.id = sp.request_id
WHERE sp.lane = 'main'
  AND sp.subagent_persona IS NOT DISTINCT FROM NULL
  AND (NULL::text IS NULL OR r.model = NULL::text)
ORDER BY sp.created_at DESC
LIMIT 20 OFFSET 0;
