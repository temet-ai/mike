SELECT l.id, l.session_uuid, l.run_id, l.seat, l.closed_at, l.close_reason
  FROM launch.launches l
 WHERE l.id = $1
