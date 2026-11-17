UPDATE launch.conditions SET asked_at = now()
 WHERE id = $1 AND asked_at IS NULL AND state = 'armed'
RETURNING id
