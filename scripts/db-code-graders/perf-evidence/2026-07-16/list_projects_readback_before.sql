SELECT
     p.project_path,
     COUNT(DISTINCT s.id)::bigint AS session_count,
     COUNT(DISTINCT r.id)::bigint AS request_count,
     p.first_seen,
     p.last_seen,
     p.pinned
 FROM projects.projects p
 LEFT JOIN sessions.sessions s ON s.project_id = p.id
 LEFT JOIN requests.requests r ON r.session_id = s.id
 GROUP BY p.id, p.project_path, p.first_seen, p.last_seen, p.pinned
 ORDER BY p.last_seen DESC;
