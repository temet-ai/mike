SELECT s.session_uuid,
       tu.tool_name,
       tu.input
  FROM sessions.sessions s
  JOIN tools.uses tu ON tu.session_id = s.id
 WHERE s.session_uuid = ANY($1::uuid[])
   AND tu.tool_name = ANY($2::text[])
   AND tu.created_at >= $3
 ORDER BY tu.created_at DESC, tu.id DESC
 LIMIT $4
