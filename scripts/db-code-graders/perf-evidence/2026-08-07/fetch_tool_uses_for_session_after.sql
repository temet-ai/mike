SELECT tu.id AS tu_id,
       tu.request_id,
       s.session_uuid::text AS session_id,
       tu.created_at AS created_at,
       tu.wire_tool_use_id AS tool_id,
       tu.tool_name,
       tu.input,
       tu.file_path,
       tu.ord,
       r.agent_instance_id,
       a.name AS agent_name,
       r.subagent_persona
  FROM tools.uses tu
  JOIN sessions.sessions s ON s.id = tu.session_id
  LEFT JOIN requests.requests r ON r.id = tu.request_id
  LEFT JOIN agents.instances ai ON ai.id = r.agent_instance_id
  LEFT JOIN agents.agents a ON a.id = ai.agent_id
 WHERE tu.session_id = 675809
   AND (NULL::text IS NULL OR a.name = NULL OR r.subagent_persona = NULL)
 ORDER BY tu.created_at DESC, tu.ord DESC
 OFFSET 0 LIMIT 200
