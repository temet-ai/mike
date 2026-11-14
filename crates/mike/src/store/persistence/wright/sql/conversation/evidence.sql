SELECT jsonb_build_object(
           'problem_id', p.id, 'check', p.check_name, 'source', p.source, 'scope', p.scope,
           'resource', p.resource, 'state', p.state, 'opened_at', p.opened_at,
           'last_seen_at', p.last_seen_at, 'healthy_at', p.healthy_at,
           'named_by_operator', p.id = ANY($1),
           'episodes', COALESCE(e.episodes, '[]'::jsonb))
  FROM wright.problems p
  LEFT JOIN LATERAL (
      SELECT jsonb_agg(jsonb_build_object(
                 'episode_id', x.id, 'opened_at', x.opened_at, 'closed_at', x.closed_at,
                 'observations', x.observations, 'evidence', x.evidence) ORDER BY x.opened_at DESC) AS episodes
        FROM (SELECT id, opened_at, closed_at, observations, evidence
                FROM wright.episodes
               WHERE problem_id = p.id
               ORDER BY opened_at DESC, id DESC
               LIMIT 3) x
  ) e ON true
 WHERE p.id = ANY($1) OR p.state = 'open'
 ORDER BY p.id = ANY($1) DESC, p.last_seen_at DESC, p.id DESC
 LIMIT $2
