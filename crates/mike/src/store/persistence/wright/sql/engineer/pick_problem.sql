SELECT p.id AS problem_id, e.id AS episode_id
  FROM wright.problems p
  JOIN wright.episodes e ON e.problem_id = p.id AND e.closed_at IS NULL
  LEFT JOIN (
      SELECT episode_id, count(*) AS attempts
        FROM wright.investigations
       WHERE episode_id IS NOT NULL
         AND state <> 'unavailable'
       GROUP BY episode_id
  ) a ON a.episode_id = e.id
 WHERE p.state = 'open'
   AND COALESCE(a.attempts, 0) < $1
   AND NOT EXISTS (SELECT 1 FROM wright.findings f WHERE f.episode_id = e.id)
   AND NOT wright.has_enabled_remedy(p.check_name)
 ORDER BY p.opened_at, p.id
 LIMIT 1
