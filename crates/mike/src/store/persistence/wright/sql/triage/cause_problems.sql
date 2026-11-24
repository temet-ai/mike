SELECT p.id, p.check_name, p.source, p.scope, p.resource, p.state, p.opened_at, p.last_seen_at,
       p.healthy_at, ep.episodes AS episodes, ep.latest_evidence
  FROM wright.problems p
  LEFT JOIN wright.episodes oe ON oe.problem_id = p.id AND oe.closed_at IS NULL
  CROSS JOIN LATERAL (
      SELECT count(*) AS episodes,
             (array_agg(e.evidence ORDER BY e.opened_at DESC, e.id DESC))[1] AS latest_evidence
        FROM wright.episodes e WHERE e.problem_id = p.id
  ) ep
 WHERE p.cause_id = $1::bigint
   AND p.state = 'open'
 ORDER BY COALESCE(oe.observations, 0) DESC, p.id
