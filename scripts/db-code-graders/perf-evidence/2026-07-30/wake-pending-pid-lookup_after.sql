SELECT s.session_uuid AS "session_uuid!"
   FROM workspace.session_registrations r
   JOIN sessions.sessions s ON s.id = r.session_id
  WHERE r.pid = 19832
    AND r.released_at IS NULL
    AND r.reaped_at IS NULL
  ORDER BY r.registered_at DESC
  LIMIT 1
