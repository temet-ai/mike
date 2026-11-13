WITH closed AS (
    UPDATE launch.launches
       SET closed_at = COALESCE(closed_at, now()),
           close_reason = COALESCE(close_reason, $3),
           close_process_exited = CASE
               WHEN close_process_exited IS TRUE OR $4::boolean IS TRUE THEN TRUE
               WHEN close_process_exited IS FALSE OR $4::boolean IS FALSE THEN FALSE
               ELSE NULL
           END
     WHERE id = $1
       AND run_id = $2
       AND opened
    RETURNING id, session_uuid, close_process_exited
), released AS (
    UPDATE workspace.session_registrations AS registration
       SET released_at = now()
      FROM closed
      JOIN sessions.sessions AS session
        ON session.session_uuid = closed.session_uuid
     WHERE registration.session_id = session.id
       AND registration.released_at IS NULL
       AND closed.close_process_exited IS TRUE
    RETURNING registration.session_id
)
SELECT id AS "id!" FROM closed
