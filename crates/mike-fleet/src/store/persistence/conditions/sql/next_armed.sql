SELECT min(GREATEST(not_before, COALESCE(next_attempt_at, not_before))) AS "at"
  FROM launch.conditions
 WHERE machine_uuid = $1
   AND state = 'armed'
