SELECT DISTINCT ON (m.id) m.body, r.id AS request_id, r.created_at, r.claude_code_agent_id AS agent_id
  FROM requests.requests r
  JOIN requests.messages rm ON rm.request_id = r.id
  JOIN messages.messages m ON m.id = rm.message_id
 WHERE r.session_id = $1
   AND m.body::text LIKE '%task-notification>%'
 ORDER BY m.id, r.id
