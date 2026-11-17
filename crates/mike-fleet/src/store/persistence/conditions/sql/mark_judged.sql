UPDATE launch.conditions SET judged_through = $2 WHERE id = $1 AND state = 'armed'
