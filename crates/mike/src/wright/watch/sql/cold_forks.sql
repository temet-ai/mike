SELECT t.name AS template,
       f.session_uuid AS fork_session,
       f.created_at AS forked_at,
       f.model AS fork_model,
       t.model AS template_model,
       f.claude_args AS fork_args,
       t.claude_args AS template_args,
       first_req.request_id AS first_request_id,
       first_req.created_at AS first_request_at,
       first_req.prompt_tokens,
       template_last.created_at AS template_last_request_at
  FROM launch.launches f
  JOIN launch.templates t ON t.id = f.template_id
  JOIN sessions.sessions fs ON fs.session_uuid = f.session_uuid
 CROSS JOIN LATERAL (
     SELECT r.id AS request_id, r.created_at,
            coalesce(resp.input_tokens, 0) + coalesce(resp.cache_creation_input_tokens, 0) AS prompt_tokens,
            coalesce(resp.cache_read_input_tokens, 0) AS cache_read
       FROM requests.requests r
       JOIN responses.responses resp ON resp.request_id = r.id
      WHERE r.session_id = fs.id
        AND r.turn_origin_id IN (1, 6)
      ORDER BY r.created_at, r.id
      LIMIT 1
 ) first_req
  LEFT JOIN sessions.sessions ts ON ts.session_uuid = t.session_uuid
  LEFT JOIN LATERAL (
     SELECT r.created_at
       FROM requests.requests r
      WHERE r.session_id = ts.id
        AND r.created_at < first_req.created_at
      ORDER BY r.created_at DESC, r.id DESC
      LIMIT 1
 ) template_last ON true
 WHERE f.created_at >= now() - interval '24 hours'
   AND f.template_id IS NOT NULL
   AND first_req.cache_read = 0
 ORDER BY f.created_at DESC, f.id DESC
 LIMIT 200
