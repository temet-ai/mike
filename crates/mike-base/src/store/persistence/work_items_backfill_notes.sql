WITH fresh AS (
    SELECT DISTINCT rm.message_id
      FROM requests.requests r
      JOIN requests.messages rm ON rm.request_id = r.id
     WHERE r.session_id = $1
       AND r.id > $2
), notes AS (
    SELECT m.id, m.body
      FROM fresh f
      JOIN messages.messages m ON m.id = f.message_id
     WHERE m.body::text LIKE '%task-notification>%'
), firsts AS (
    SELECT n.id AS message_id, min(rm.request_id) AS request_id
      FROM notes n
      JOIN requests.messages rm ON rm.message_id = n.id
      JOIN requests.requests r ON r.id = rm.request_id
     WHERE r.session_id = $1
     GROUP BY n.id
)
SELECT n.body AS "body!", r.id AS request_id, r.created_at, r.claude_code_agent_id AS agent_id
  FROM firsts f
  JOIN notes n ON n.id = f.message_id
  JOIN requests.requests r ON r.id = f.request_id
 WHERE f.request_id > $2
 ORDER BY n.id, r.id
