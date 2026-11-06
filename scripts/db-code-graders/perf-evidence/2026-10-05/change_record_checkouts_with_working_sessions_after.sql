SELECT coalesce(s.cwd, loc.root_path) AS "checkout",
       s.project_id,
       max(r.created_at) AS "newest_request_at"
  FROM requests.requests r
  JOIN sessions.sessions s ON s.id = r.session_id
  LEFT JOIN projects.locations loc ON loc.location_uuid = s.location_uuid
 WHERE r.created_at >= '2026-10-05 19:10:00+00'
   AND coalesce(s.cwd, loc.root_path) IS NOT NULL
 GROUP BY coalesce(s.cwd, loc.root_path), s.project_id
 ORDER BY 3 DESC, 1
