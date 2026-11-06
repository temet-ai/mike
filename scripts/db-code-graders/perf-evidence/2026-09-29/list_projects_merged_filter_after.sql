SELECT p.id AS project_id, p.project_uuid, p.project_path,
       COALESCE(sess_agg.session_count, 0) AS session_count,
       COALESCE(req_agg.request_count, 0) AS request_count,
       p.first_seen, p.last_seen, p.pinned, p.ttl_override, p.pin_created_at, p.ttl_override_set_at, p.custom_name
FROM (SELECT pr.*, NULL::bigint AS merged_into_id FROM projects.projects pr) p
LEFT JOIN (SELECT project_id, COUNT(*) AS session_count FROM sessions.sessions GROUP BY project_id) sess_agg ON sess_agg.project_id = p.id
LEFT JOIN (SELECT s.project_id, COUNT(r.id) AS request_count FROM sessions.sessions s JOIN requests.requests r ON r.session_id = s.id GROUP BY s.project_id) req_agg ON req_agg.project_id = p.id
WHERE p.project_path <> '(no project)' AND p.merged_into_id IS NULL
ORDER BY p.last_seen DESC
