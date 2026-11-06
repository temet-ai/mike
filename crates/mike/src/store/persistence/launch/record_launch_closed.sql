WITH closed AS (
    UPDATE launch.launches
       SET closed_at            = COALESCE(closed_at, now()),
           close_reason         = COALESCE(close_reason, $2),
           close_process_exited = COALESCE(close_process_exited, $3)
     WHERE id = $1
    RETURNING id, session_uuid
),
released AS (
    UPDATE workspace.session_registrations r
       SET released_at = now()
      FROM closed c
      JOIN sessions.sessions s ON s.session_uuid = c.session_uuid
     WHERE r.session_id = s.id
       AND r.released_at IS NULL
    RETURNING r.session_id
)
SELECT id AS "id!" FROM closed
