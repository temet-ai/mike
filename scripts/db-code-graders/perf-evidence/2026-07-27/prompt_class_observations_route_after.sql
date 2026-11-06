SELECT request_id, session_id, created_at, content_sha256, altered,
       COUNT(*) OVER ()::bigint AS "total_count!"
FROM system_prompts.session_prompts
WHERE lane = 'subagent'
  AND subagent_persona IS NOT DISTINCT FROM 'Rust Development Expert'
ORDER BY created_at DESC
LIMIT 20 OFFSET 0;
