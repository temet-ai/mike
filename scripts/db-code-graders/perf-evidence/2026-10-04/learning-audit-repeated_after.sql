SELECT coalesce(p.project_path,'(unresolved)') AS project,u.tool_name,
          md5(u.input::text) AS input_fingerprint,a.content_sha256 AS result_fingerprint,
          count(*) AS calls,count(DISTINCT r.session_id) AS sessions,
          min(u.id) AS example_tool_use_id,min(r.id) AS example_request_id,
          sum(a.content_bytes)-max(a.content_bytes) AS repeated_result_bytes
          FROM requests.requests r JOIN tools.uses u ON u.request_id=r.id
          JOIN tools.results a ON a.tool_use_id=u.id AND NOT a.is_error
          LEFT JOIN sessions.sessions s ON s.id=r.session_id
          LEFT JOIN projects.projects p ON p.id=coalesce(r.project_id,s.project_id)
          WHERE r.created_at >= '2026-09-27T17:40:00Z' AND r.created_at < '2026-10-04T17:40:00Z' GROUP BY 1,2,3,4 HAVING count(*) >= 3
          ORDER BY repeated_result_bytes DESC NULLS LAST,1,2,3,4