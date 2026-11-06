SELECT ad.tool_use_id AS "tool_use_id",
       tu.created_at AS "dispatched_at",
       COALESCE(tu.input->>'prompt', '') AS "prompt",
       s.session_uuid AS "session_uuid",
       r.id AS "request_id"
  FROM agents.dispatches ad
  JOIN tools.uses tu ON tu.id = ad.tool_use_id
  JOIN requests.requests r ON r.id = tu.request_id
  JOIN sessions.sessions s ON s.id = r.session_id
 WHERE ad.agent_id = 6
 ORDER BY tu.created_at DESC, ad.id DESC
 LIMIT 50 OFFSET 0;
