SELECT s.session_uuid
  FROM workspace.session_registrations r
  JOIN sessions.sessions s ON s.id = r.session_id
 WHERE r.released_at IS NULL
   AND r.lease_expires_at > now()
   AND NOT EXISTS (
       SELECT 1 FROM launch.launches l WHERE l.session_uuid = s.session_uuid AND l.run_id IS NOT NULL)
 ORDER BY s.last_seen DESC, s.id DESC
 LIMIT 1
