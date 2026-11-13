SELECT u.file_path,
       s.session_uuid::text AS session_uuid,
       u.request_id,
       u.tool_name,
       u.created_at,
       a.name AS agent_name,
       r.subagent_persona
  FROM tools.uses u
  JOIN requests.requests r ON r.id = u.request_id
  JOIN sessions.sessions s ON s.id = r.session_id
  LEFT JOIN agents.instances ai ON ai.id = r.agent_instance_id
  LEFT JOIN agents.agents a ON a.id = ai.agent_id
 WHERE u.operation = 'write'
   AND u.file_path = ANY(ARRAY['/Users/vimes/projects/temet/temet-proxy/crates/mike/src/config/mod.rs','/Users/vimes/projects/temet/temet-proxy/crates/mike-base/src/settings/mod.rs','/Users/vimes/projects/temet/temet-proxy/crates/mike/src/steerer/stage.rs'])
   AND ('2026-08-05T11:40:00Z'::timestamptz IS NULL OR u.created_at >= '2026-08-05T11:40:00Z'::timestamptz)
   AND ('2026-08-05T18:00:00Z'::timestamptz IS NULL OR u.created_at <  '2026-08-05T18:00:00Z'::timestamptz)
 ORDER BY u.file_path, u.created_at, u.id
