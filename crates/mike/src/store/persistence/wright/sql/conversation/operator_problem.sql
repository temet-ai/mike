SELECT jsonb_build_object('problem_id', p.id, 'state', p.state, 'healthy_at', p.healthy_at)
  FROM wright.problems p
 WHERE p.check_name = $1 AND p.scope = 'operator' AND p.resource = $2
