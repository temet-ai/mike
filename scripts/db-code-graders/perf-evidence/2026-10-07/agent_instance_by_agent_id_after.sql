SELECT ai.id
  FROM agents.instances ai
  JOIN requests.requests first_req ON first_req.id = ai.first_request_id
 WHERE ai.session_id = 1158718
   AND first_req.claude_code_agent_id = 'aae2cc315597a51a3'
 ORDER BY ai.id
 LIMIT 1
