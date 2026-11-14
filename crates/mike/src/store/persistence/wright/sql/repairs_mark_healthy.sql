UPDATE wright.repairs r
   SET state = 'healthy', healthy_at = p.healthy_at, updated_at = now()
  FROM wright.problems p
 WHERE r.state = 'deployed'
   AND p.id = r.problem_id
   AND p.state = 'healthy'
   AND p.healthy_at > r.deployed_at
RETURNING r.id
