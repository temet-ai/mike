SELECT ai.id
  FROM agents.instances ai
 WHERE ai.session_id      = 1158718
   AND ai.subagent_persona = 'general-purpose'
 ORDER BY (ai.first_request_id IS NULL),
          ai.first_request_id DESC,
          ai.id DESC
 LIMIT 1
