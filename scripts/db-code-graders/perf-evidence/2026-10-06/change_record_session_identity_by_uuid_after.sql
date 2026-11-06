SELECT s.id, s.session_uuid AS uuid, g.display_name AS name, s.cwd, s.project_id
  FROM sessions.sessions s
  LEFT JOIN sessions.session_goals g ON g.session_id = s.id
 WHERE s.session_uuid = 'c6a60e46-0235-4b1f-a15f-ca42aac84b6d'::uuid
