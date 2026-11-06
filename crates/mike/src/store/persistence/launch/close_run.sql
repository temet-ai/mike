UPDATE launch.runs
   SET closed_at = COALESCE(closed_at, now())
 WHERE run_id = $1
RETURNING run_id
