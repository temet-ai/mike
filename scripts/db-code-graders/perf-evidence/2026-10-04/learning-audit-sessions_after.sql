SELECT coalesce(p.project_path,'(unresolved)') AS project,
          grouping(p.project_path)::boolean AS is_global,
          count(DISTINCT r.session_id) AS sessions
          FROM requests.requests r LEFT JOIN sessions.sessions s ON s.id=r.session_id
          LEFT JOIN projects.projects p ON p.id=coalesce(r.project_id,s.project_id)
          WHERE r.created_at >= '2026-09-27T17:40:00Z' AND r.created_at < '2026-10-04T17:40:00Z' GROUP BY GROUPING SETS ((p.project_path),());
