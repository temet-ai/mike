SELECT n.title AS "title!", n.data AS "data!", n.created_at AS "created_at!",
       s.id AS "session_id!", s.session_uuid AS "session_uuid!", s.project_id AS "project_id!",
       nullif(btrim(sg.role), '') AS role
  FROM workspace.session_notes n
  JOIN sessions.sessions s ON s.id = n.session_id
  LEFT JOIN sessions.session_goals sg ON sg.session_id = s.id
 WHERE n.note_uuid = $1
