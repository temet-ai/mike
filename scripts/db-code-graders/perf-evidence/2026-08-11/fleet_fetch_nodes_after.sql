SELECT s.id, s.session_uuid, s.title, s.last_status, s.last_seen
     FROM sessions.sessions s
    WHERE s.project_id = 313870
      AND (
          s.last_seen >= now() - make_interval(hours => 24)
          OR EXISTS (
              SELECT 1 FROM workspace.session_registrations wr
               WHERE wr.session_id = s.id
                 AND wr.released_at IS NULL
                 AND wr.lease_expires_at >= now()
          )
      )
    ORDER BY s.last_seen DESC;
