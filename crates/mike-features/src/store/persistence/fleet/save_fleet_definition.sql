UPDATE launch.fleets
   SET definition = $2,
       missing    = $3,
       updated_at = now()
 WHERE fleet_uuid = $1
   AND updated_at = $4
RETURNING updated_at
