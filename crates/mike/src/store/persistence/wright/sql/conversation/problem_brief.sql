SELECT jsonb_build_object('problem_id', id, 'state', state, 'healthy_at', healthy_at)
  FROM wright.problems
 WHERE id = $1
