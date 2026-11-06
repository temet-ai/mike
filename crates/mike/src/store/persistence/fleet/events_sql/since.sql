SELECT id, run_id, task, session_uuid, kind, payload, occurred_at
  FROM launch.fleet_events
 WHERE run_id = $1
   AND id > $2
 ORDER BY id
