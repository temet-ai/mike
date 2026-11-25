UPDATE wright.repairs r
   SET state = 'healthy', healthy_at = GREATEST(p.healthy_at, r.deployed_at), updated_at = now()
  FROM wright.problems p
 WHERE r.state = 'deployed'
   AND p.id = r.problem_id
   AND p.state = 'healthy'
   AND GREATEST(p.healthy_at, r.deployed_at) < now() - interval '24 hours'
   AND EXISTS (
       SELECT 1
         FROM wright.check_runs c
        WHERE c.check_name = p.check_name
          AND c.finished_at > r.deployed_at
          AND c.outcome <> 'unknown')
RETURNING r.id
