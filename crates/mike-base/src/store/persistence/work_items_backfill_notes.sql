WITH firsts AS (
    SELECT rm.message_id, min(rm.request_id) AS request_id
      FROM requests.requests r
      JOIN requests.messages rm ON rm.request_id = r.id
     WHERE r.session_id = $1
     GROUP BY rm.message_id
)
SELECT m.body, r.id AS request_id, r.created_at, r.claude_code_agent_id AS agent_id
  FROM firsts f
  JOIN messages.messages m ON m.id = f.message_id
  JOIN requests.requests r ON r.id = f.request_id
 WHERE m.body::text LIKE '%task-notification>%'
 ORDER BY m.id, r.id
