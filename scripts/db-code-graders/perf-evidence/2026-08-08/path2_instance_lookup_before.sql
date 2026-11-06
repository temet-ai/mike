SELECT ai.id
  FROM agents.instances ai
 WHERE ai.session_id       = 691123
   AND ai.subagent_persona  = 'subagent spawned by a workflow orchestration script'
   AND ai.first_request_id IS NOT NULL
 ORDER BY ai.first_request_id DESC
 LIMIT 1
