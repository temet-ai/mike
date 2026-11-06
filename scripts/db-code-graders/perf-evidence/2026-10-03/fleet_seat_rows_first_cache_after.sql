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
           AND r.turn_origin_id IN (SELECT o.id FROM requests.turn_origins o
                                     WHERE o.code IN ('primary', 'primary_first'))
           AND rs.input_tokens IS NOT NULL
           AND r.created_at BETWEEN l.created_at - interval '1 hour' AND now()
         ORDER BY r.id DESC
         LIMIT 1)::bigint AS context_tokens,
       (SELECT rs.response_status
          FROM requests.requests r
          JOIN responses.responses rs ON rs.request_id = r.id
         WHERE r.session_id = s.id
           AND r.subagent_persona IS NULL
           AND r.turn_origin_id IN (SELECT o.id FROM requests.turn_origins o
                                     WHERE o.code IN ('primary', 'primary_first'))
           AND r.created_at BETWEEN l.created_at - interval '1 hour' AND now()
         ORDER BY r.id DESC
         LIMIT 1) AS last_status,
       (l.template_id IS NOT NULL) AS forked,
       COALESCE(fr.cache_read_input_tokens, 0)::bigint AS first_cache_read,
       COALESCE(fr.cache_creation_input_tokens, 0)::bigint AS first_cache_written
  FROM launch.launches l
  LEFT JOIN sessions.sessions s        ON s.session_uuid = l.session_uuid
  LEFT JOIN sessions.session_goals g   ON g.session_id = s.id
  LEFT JOIN LATERAL (
        SELECT count(*) AS requests, max(r.created_at) AS last_request_at,
               min(r.id) FILTER (WHERE r.subagent_persona IS NULL
                                   AND r.turn_origin_id IN (SELECT o.id FROM requests.turn_origins o
                                                             WHERE o.code IN ('primary', 'primary_first'))) AS first_id
          FROM requests.requests r
         WHERE r.session_id = s.id
           AND r.created_at BETWEEN l.created_at - interval '1 hour' AND now()
       ) rq ON true
  LEFT JOIN responses.responses fr ON fr.request_id = rq.first_id
 WHERE l.run_id = 'fleet-holdfast-wear-and-routine-32a3aefc'
   AND l.opened
 ORDER BY l.created_at, l.id
