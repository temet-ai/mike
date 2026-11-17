UPDATE launch.conditions
   SET state = 'cancelled'
 WHERE condition_uuid = $1 AND state = 'armed'
