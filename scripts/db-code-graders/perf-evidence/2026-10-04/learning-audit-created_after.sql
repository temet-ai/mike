SELECT coalesce(p.project_path,'(unresolved)') AS project,
          u.input->>'file_path' AS file_path,count(*) AS writes,
          count(DISTINCT r.session_id) AS sessions,min(r.id) AS example_request_id
          FROM requests.requests r JOIN tools.uses u ON u.request_id=r.id
          LEFT JOIN sessions.sessions s ON s.id=r.session_id
          LEFT JOIN projects.projects p ON p.id=coalesce(r.project_id,s.project_id)
          WHERE r.created_at >= '2026-09-27T17:40:00Z' AND r.created_at < '2026-10-04T17:40:00Z' AND u.tool_name IN ('Write','write_file')
          AND u.input->>'file_path' ~ '(scripts/|\.(py|sh|bash|js|ts)$)'
          GROUP BY 1,2 HAVING count(DISTINCT r.session_id)>1 ORDER BY writes DESC,1,2