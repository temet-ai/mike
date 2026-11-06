SELECT coalesce(p.project_path,'(unresolved)') AS project,
          o.code AS origin,r.model,count(*) AS requests,count(DISTINCT r.session_id) AS sessions,
          sum(a.input_tokens) AS input_tokens,sum(a.output_tokens) AS output_tokens,
          sum(a.cache_read_input_tokens) AS cache_read_tokens,
          sum(a.cache_creation_input_tokens) AS cache_write_tokens,
          count(*) FILTER(WHERE a.input_tokens IS NULL) AS missing_usage
          FROM requests.requests r LEFT JOIN responses.responses a ON a.request_id=r.id
          LEFT JOIN sessions.sessions s ON s.id=r.session_id
          LEFT JOIN projects.projects p ON p.id=coalesce(r.project_id,s.project_id)
          JOIN requests.turn_origins o ON o.id=r.turn_origin_id WHERE r.created_at >= '2026-09-27T17:40:00Z' AND r.created_at < '2026-10-04T17:40:00Z'
          GROUP BY 1,2,3 ORDER BY 1,2,3