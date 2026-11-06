SELECT COUNT(*)
  FROM tools.uses tu
  LEFT JOIN requests.requests r ON r.id = tu.request_id
  LEFT JOIN agents.instances ai ON ai.id = r.agent_instance_id
  LEFT JOIN agents.agents a ON a.id = ai.agent_id
 WHERE tu.session_id = 675809
   AND (NULL::text IS NULL OR a.name = NULL OR r.subagent_persona = NULL)
