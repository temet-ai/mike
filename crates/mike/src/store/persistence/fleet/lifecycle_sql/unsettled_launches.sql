SELECT id, session_uuid, backend, seat, surface_ref, pid, opened, result, opened_at, closed_at,
       close_process_exited
  FROM launch.launches
 WHERE run_id = $1
   AND ((opened AND (closed_at IS NULL OR close_process_exited IS DISTINCT FROM TRUE))
        OR (NOT opened AND result <> 'failed'))
 ORDER BY created_at, id
