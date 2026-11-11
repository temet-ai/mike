UPDATE launch.launches AS launch
   SET closed_at = COALESCE(launch.closed_at, now()),
       close_reason = COALESCE(launch.close_reason, $3),
       close_process_exited = TRUE,
       result = 'failed',
       reason = COALESCE(launch.reason, 'never_started')
 WHERE launch.id = $1
   AND launch.run_id = $2
   AND launch.opened_at IS NULL
   AND launch.surface_ref IS NULL
   AND launch.created_at < now() - make_interval(secs => $4)
   AND (
       NOT EXISTS (
           SELECT 1
             FROM sessions.sessions AS session
             JOIN requests.requests AS request ON request.session_id = session.id
            WHERE session.session_uuid = launch.session_uuid
       )
       OR EXISTS (
           SELECT 1
             FROM sessions.sessions AS session
             JOIN workspace.session_registrations AS registration ON registration.session_id = session.id
            WHERE session.session_uuid = launch.session_uuid
              AND registration.released_at IS NOT NULL
       )
   )
RETURNING launch.id
