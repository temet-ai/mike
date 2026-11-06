SELECT r.claude_code_agent_id, res.created_at, res.content
      FROM tools.uses u
      JOIN tools.results res ON res.tool_use_id = u.id
      JOIN requests.requests r ON r.id = u.request_id
     WHERE u.session_id = 1545259
       AND u.tool_name = 'Bash'
       AND (u.input->>'command' LIKE '%git push%' OR u.input->>'command' LIKE '%git commit%')
     ORDER BY res.created_at
