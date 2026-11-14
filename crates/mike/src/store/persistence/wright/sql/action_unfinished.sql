SELECT a.id AS action_id, r.name AS remedy, r.action, r.params, r.max_resources,
       a.problem_id, a.episode_id, p.scope, p.resource, a.state, a.prior_state, a.effect
  FROM wright.actions a
  JOIN wright.remedies r ON r.name = a.remedy
  JOIN wright.problems p ON p.id = a.problem_id
 WHERE a.state IN ('claimed', 'applied')
   AND a.updated_at < now() - make_interval(secs => $1::double precision)
 ORDER BY a.id
 LIMIT $2
