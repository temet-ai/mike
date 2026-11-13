SELECT fleet_uuid, state, run_id AS "run_id!"
  FROM launch.fleets
 WHERE run_id = $1
 ORDER BY created_at, id
 FOR KEY SHARE
