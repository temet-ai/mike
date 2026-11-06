SELECT DISTINCT s.session_uuid
  FROM workspace.session_registrations r
  JOIN sessions.sessions s ON s.id = r.session_id
  JOIN sessions.session_goals sg ON sg.session_id = s.id
 WHERE r.project_id = 313870
   AND sg.role = 'rust-developer'
   AND r.released_at IS NULL
   AND r.lease_expires_at > now()
