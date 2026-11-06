SELECT ag.name
  FROM agents.instances ai
  JOIN agents.agents ag ON ag.id = ai.agent_id
 WHERE ai.session_id = 1
 ORDER BY ai.created_at DESC
 LIMIT 1;
