SELECT p.id AS project_id,
       COALESCE(sess_agg.session_count, 0) AS session_count,
       COALESCE(req_agg.request_count, 0) AS request_count
FROM projects.projects p
LEFT JOIN (SELECT project_id, COUNT(*) AS session_count FROM sessions.sessions GROUP BY project_id) sess_agg ON sess_agg.project_id = p.id
LEFT JOIN (SELECT s.project_id, COUNT(r.id) AS request_count FROM sessions.sessions s JOIN requests.requests r ON r.session_id = s.id GROUP BY s.project_id) req_agg ON req_agg.project_id = p.id
WHERE p.project_path <> '(no project)' AND p.merged_into_id IS NULL
