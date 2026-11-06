SELECT l.seat,
       g.role,
       l.agent,
       l.model,
       l.session_uuid,
       l.surface_ref,
       l.opened_at,
       l.closed_at,
       COALESCE(rq.requests, 0)::bigint AS requests,
       rq.last_request_at,
       (SELECT rs.input_tokens
               + COALESCE(rs.cache_read_input_tokens, 0)
               + COALESCE(rs.cache_creation_input_tokens, 0)
          FROM requests.requests r
          JOIN responses.responses rs ON rs.request_id = r.id
         WHERE r.session_id = s.id
           AND r.subagent_persona IS NULL
           AND rs.input_tokens IS NOT NULL
           AND r.created_at BETWEEN l.created_at - interval '1 hour' AND now()
         ORDER BY r.id DESC
         LIMIT 1)::bigint AS context_tokens
  FROM launch.launches l
  LEFT JOIN sessions.sessions s        ON s.session_uuid = l.session_uuid
  LEFT JOIN sessions.session_goals g   ON g.session_id = s.id
  LEFT JOIN LATERAL (
        SELECT count(*) AS requests, max(r.created_at) AS last_request_at
          FROM requests.requests r
         WHERE r.session_id = s.id
           AND r.created_at BETWEEN l.created_at - interval '1 hour' AND now()
       ) rq ON true
 WHERE l.run_id = 'fleet-judgement-at-work-541ce7e3'
   AND l.opened
 ORDER BY l.created_at, l.id
