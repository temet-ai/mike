SELECT p.id, p.check_name, p.source, p.scope, p.resource, p.state, p.opened_at, p.last_seen_at,
       p.healthy_at, ep.episodes AS episodes, ep.latest_evidence
  FROM wright.problems p
  CROSS JOIN LATERAL (
      SELECT count(*) AS episodes,
             (array_agg(e.evidence ORDER BY e.opened_at DESC, e.id DESC))[1] AS latest_evidence
        FROM wright.episodes e WHERE e.problem_id = p.id
  ) ep
 WHERE ($1::text IS NULL OR p.state = $1)
   AND ($2::text IS NULL OR p.check_name = $2)
   AND ($3::text IS NULL OR p.source = $3)
 ORDER BY p.last_seen_at DESC, p.id DESC
 LIMIT $4 OFFSET $5
