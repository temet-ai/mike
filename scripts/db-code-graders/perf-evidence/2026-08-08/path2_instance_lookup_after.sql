SELECT ai.id
  FROM agents.instances ai
 WHERE ai.session_id      = 691123
   AND ai.subagent_persona = 'subagent spawned by a workflow orchestration script'
 ORDER BY (ai.first_request_id IS NULL),
          ai.first_request_id DESC,
          ai.id DESC
 LIMIT 1
