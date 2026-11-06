SELECT s.id, s.session_uuid AS uuid, g.display_name AS name, s.cwd, s.project_id
  FROM sessions.sessions s
  LEFT JOIN sessions.session_goals g ON g.session_id = s.id
 WHERE s.id = ANY('{1348628,1348629,1348630,1348631}'::bigint[])
 ORDER BY s.id
