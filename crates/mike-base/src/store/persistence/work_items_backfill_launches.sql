SELECT u.wire_tool_use_id AS "wire_tool_use_id!",
       u.tool_name AS "tool_name!",
       u.input->>'description' AS "description?",
       res.content AS "content!",
       res.request_id AS "request_id!",
       u.created_at AS "created_at!",
       r.claude_code_agent_id AS "agent_id?"
  FROM tools.results res
  JOIN tools.uses u ON u.id = res.tool_use_id
  JOIN requests.requests r ON r.id = res.request_id
 WHERE res.session_id = $1
   AND u.tool_name IN ('Bash', 'Agent', 'Task')
   AND (res.content::text LIKE '%Command running in background with ID%'
        OR res.content::text LIKE '%Async agent launched successfully%')
 ORDER BY res.request_id
