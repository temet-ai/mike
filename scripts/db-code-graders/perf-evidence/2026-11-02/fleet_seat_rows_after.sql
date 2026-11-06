WITH primary_origins AS MATERIALIZED (
    SELECT o.id FROM requests.turn_origins o WHERE o.code IN ('primary', 'primary_first')
)
SELECT l.seat,
       g.role,
       l.agent,
       l.model,
       l.session_uuid,
       l.surface_ref,
       l.opened_at,
       l.closed_at,
       COALESCE(rq.requests, 0)::bigint,
       rq.last_request_at,
       ctx.context_tokens,
       ls.response_status AS last_status,
       (l.template_id IS NOT NULL),
       COALESCE(fr.cache_read_input_tokens, 0)::bigint AS first_cache_read,
       COALESCE(fr.cache_creation_input_tokens, 0)::bigint AS first_cache_written
  FROM launch.launches l
  LEFT JOIN sessions.sessions s        ON s.session_uuid = l.session_uuid
  LEFT JOIN sessions.session_goals g   ON g.session_id = s.id
  LEFT JOIN LATERAL (
        SELECT count(*) AS requests, max(r.created_at) AS last_request_at
          FROM requests.requests r
         WHERE r.session_id = s.id
           AND r.created_at BETWEEN l.created_at - interval '1 hour' AND now()
       ) rq ON true
  LEFT JOIN LATERAL (
        SELECT r.id
          FROM requests.requests r
         WHERE r.session_id = s.id
           AND r.subagent_persona IS NULL
           AND r.turn_origin_id IN (SELECT p.id FROM primary_origins p)
           AND r.created_at BETWEEN l.created_at - interval '1 hour' AND now()
         ORDER BY r.created_at, r.id
         LIMIT 1
       ) fq ON true
  LEFT JOIN responses.responses fr ON fr.request_id = fq.id
  LEFT JOIN LATERAL (
        SELECT (rs.input_tokens
               + COALESCE(rs.cache_read_input_tokens, 0)
               + COALESCE(rs.cache_creation_input_tokens, 0))::bigint AS context_tokens
          FROM requests.requests r
          JOIN responses.responses rs ON rs.request_id = r.id
         WHERE r.session_id = s.id
           AND r.subagent_persona IS NULL
           AND r.turn_origin_id IN (SELECT p.id FROM primary_origins p)
           AND rs.input_tokens IS NOT NULL
           AND r.created_at BETWEEN l.created_at - interval '1 hour' AND now()
         ORDER BY r.created_at DESC, r.id DESC
         LIMIT 1
       ) ctx ON true
  LEFT JOIN LATERAL (
        SELECT rs.response_status
          FROM requests.requests r
          JOIN responses.responses rs ON rs.request_id = r.id
         WHERE r.session_id = s.id
           AND r.subagent_persona IS NULL
           AND r.turn_origin_id IN (SELECT p.id FROM primary_origins p)
           AND r.created_at BETWEEN l.created_at - interval '1 hour' AND now()
         ORDER BY r.created_at DESC, r.id DESC
         LIMIT 1
       ) ls ON true
 WHERE l.run_id = 'fleet-ink-migration-2-b870a477'
   AND l.opened
 ORDER BY l.created_at, l.id
