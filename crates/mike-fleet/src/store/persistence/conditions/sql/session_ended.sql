SELECT EXISTS (
    SELECT 1
      FROM workspace.session_registrations r
      JOIN sessions.sessions s ON s.id = r.session_id
     WHERE s.session_uuid = $1
       AND (r.released_at >= $2
            OR r.reaped_at >= $2
            OR r.lease_expires_at BETWEEN $2 AND now())
) AS "holds!"
