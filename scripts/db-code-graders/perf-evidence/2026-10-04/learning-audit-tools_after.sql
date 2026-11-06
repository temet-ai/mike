SELECT coalesce(p.project_path,'(unresolved)') AS project,u.tool_name,
          count(*) AS calls,count(*) FILTER(WHERE a.is_error) AS errors,
          sum(a.content_bytes) AS result_bytes,count(*) FILTER(WHERE a.tool_use_id IS NULL) AS missing_results
          FROM requests.requests r JOIN tools.uses u ON u.request_id=r.id
          LEFT JOIN tools.results a ON a.tool_use_id=u.id
          LEFT JOIN sessions.sessions s ON s.id=r.session_id
          LEFT JOIN projects.projects p ON p.id=coalesce(r.project_id,s.project_id)
          WHERE r.created_at >= '2026-09-27T17:40:00Z' AND r.created_at < '2026-10-04T17:40:00Z' GROUP BY 1,2 ORDER BY calls DESC,1,2