SELECT s.session_uuid,
       tu.tool_name,
       tu.input
  FROM sessions.sessions s
  JOIN tools.uses tu ON tu.session_id = s.id
 WHERE s.session_uuid = ANY('{d1aad222-2d68-4170-9a3c-0bf20eca2e3c,dc9fe585-ed47-4f90-bbb6-e9cd70fc901b}'::uuid[])
   AND tu.tool_name = ANY('{Read,Grep,Glob}'::text[])
   AND tu.created_at >= now() - interval '5 minutes'
 ORDER BY tu.created_at DESC, tu.id DESC
 LIMIT 500
