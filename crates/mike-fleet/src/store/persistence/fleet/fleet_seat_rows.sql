SELECT l.seat,
       g.role,
       l.agent,
       COALESCE(observation.observed_model, l.model, observation.requested_model) AS model,
       l.session_uuid,
       l.surface_ref,
       l.opened_at,
       l.closed_at,
       COALESCE(rq.requests, 0)::bigint AS "requests!",
       COALESCE(observation.observed_at, rq.last_request_at) AS last_request_at,
       ctx.context_tokens AS "context_tokens?",
       ls.response_status AS "last_status?",
       (l.template_id IS NOT NULL) AS "forked!",
       COALESCE(fq.cache_read_input_tokens, 0)::bigint AS first_cache_read,
       COALESCE(fq.cache_creation_input_tokens, 0)::bigint AS first_cache_written
  FROM launch.launches l
  LEFT JOIN sessions.sessions s        ON s.session_uuid = l.session_uuid
  LEFT JOIN sessions.session_goals g   ON g.session_id = s.id
  LEFT JOIN runtime.session_observation_v observation ON observation.session_id = s.id
  LEFT JOIN LATERAL (
        SELECT count(*) AS requests, max(r.created_at) AS last_request_at
          FROM runtime.model_call_facts r
         WHERE r.session_id = s.id
           AND r.created_at BETWEEN l.created_at - interval '1 hour' AND now()
       ) rq ON true
  LEFT JOIN LATERAL (
        SELECT r.id, r.cache_read_input_tokens, r.cache_creation_input_tokens
          FROM runtime.model_call_facts r
         WHERE r.session_id = s.id
           AND r.primary_lane
           AND r.created_at BETWEEN l.created_at - interval '1 hour' AND now()
         ORDER BY r.created_at, r.id
         LIMIT 1
       ) fq ON true
  LEFT JOIN LATERAL (
        SELECT r.context_tokens
          FROM runtime.model_call_facts r
          WHERE r.session_id = s.id
           AND r.primary_lane
           AND r.input_tokens IS NOT NULL
           AND r.created_at BETWEEN l.created_at - interval '1 hour' AND now()
         ORDER BY r.created_at DESC, r.id DESC
         LIMIT 1
       ) ctx ON true
  LEFT JOIN LATERAL (
        SELECT r.response_status
          FROM runtime.model_call_facts r
          WHERE r.session_id = s.id
           AND r.primary_lane
           AND r.created_at BETWEEN l.created_at - interval '1 hour' AND now()
         ORDER BY r.created_at DESC, r.id DESC
         LIMIT 1
       ) ls ON true
 WHERE l.run_id = $1
   AND l.opened
 ORDER BY l.created_at, l.id
