SELECT COALESCE(sa.project_id, ra.project_id) AS project_id,
       COALESCE(sa.session_count, 0) AS session_count,
       COALESCE(ra.request_count, 0) AS request_count
FROM (SELECT project_id, COUNT(*) AS session_count FROM sessions.sessions GROUP BY project_id) sa
FULL JOIN (SELECT s.project_id, COUNT(r.id) AS request_count FROM sessions.sessions s JOIN requests.requests r ON r.session_id = s.id GROUP BY s.project_id) ra ON ra.project_id = sa.project_id
