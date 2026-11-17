SELECT s.session_uuid AS "session_uuid!",
       s.project_id AS "project_id!",
       s.last_seen AS "last_request_at!",
       COALESCE(s.cwd, r.cwd) AS cwd,
       COALESCE(r.session_id IS NOT NULL
                AND r.released_at IS NULL
                AND r.reaped_at IS NULL
                AND r.lease_expires_at > now(), false) AS "live!"
  FROM sessions.sessions s
  LEFT JOIN workspace.session_registrations r ON r.session_id = s.id
 WHERE s.session_uuid = $1
