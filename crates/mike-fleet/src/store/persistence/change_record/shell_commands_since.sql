SELECT u.id AS "id!",
       left(u.input->>'command', 8000) AS "command!",
       s.cwd AS "cwd!",
       s.project_id AS "project_id!",
       s.session_uuid AS "session!",
       u.created_at AS "at!"
  FROM tools.uses u
  JOIN sessions.sessions s ON s.id = u.session_id
 WHERE u.created_at >= $1
   AND u.id > $2
   AND u.tool_name = 'Bash'
   AND u.input->>'command' IS NOT NULL
   AND s.cwd IS NOT NULL
   AND s.project_id IS NOT NULL
 ORDER BY u.id
