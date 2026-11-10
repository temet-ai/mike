WITH matched AS (
    SELECT e.id, u.session_id, u.request_id
      FROM memory.search_events e
      CROSS JOIN LATERAL (
          SELECT u.session_id, u.request_id
            FROM tools.uses u
           WHERE u.created_at BETWEEN e.created_at - make_interval(secs => 60) AND e.created_at
             AND u.tool_name LIKE '%' || e.tool
             AND u.input ->> 'query' = e.query
             AND u.session_id IS NOT NULL
           ORDER BY u.created_at DESC, u.id DESC
           LIMIT 1
      ) u
     WHERE e.session_id IS NULL
       AND e.created_at > now() - make_interval(secs => 86400)
     ORDER BY e.id
     LIMIT 250
)
UPDATE memory.search_events e
   SET session_id = m.session_id, request_id = m.request_id
  FROM matched m
 WHERE e.id = m.id
RETURNING e.id
