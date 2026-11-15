SELECT f.id AS followup_id, lp.problem_id, lp.episode_id, f.check_name, f.title
  FROM wright.followups f
  JOIN LATERAL (
      SELECT p.id AS problem_id, e.id AS episode_id
        FROM wright.problems p
        JOIN wright.episodes e ON e.problem_id = p.id AND e.closed_at IS NULL
       WHERE p.check_name = f.check_name AND p.state = 'open'
       ORDER BY p.id DESC
       LIMIT 1
  ) lp ON true
  LEFT JOIN (
      SELECT followup_id, count(*) AS attempts
        FROM wright.investigations
       WHERE followup_id IS NOT NULL
       GROUP BY followup_id
  ) a ON a.followup_id = f.id
 WHERE f.kind = 'root_cause'
   AND f.state = 'open'
   AND COALESCE(a.attempts, 0) < $1::bigint
   AND NOT EXISTS (SELECT 1 FROM wright.followup_findings ff WHERE ff.followup_id = f.id)
 ORDER BY f.id
 LIMIT 1
