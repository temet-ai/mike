UPDATE workspace.session_registrations r
   SET lease_expires_at = now() + make_interval(mins => $2::int)
  FROM remote_control.channels c
 WHERE c.cse_id = $1
   AND c.session_id = r.session_id
   AND r.released_at IS NULL
   AND r.lease_expires_at < now() + make_interval(mins => $3::int)
