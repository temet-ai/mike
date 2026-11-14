WITH placed AS (
    SELECT coalesce(s.cwd, loc.root_path) AS checkout,
           s.project_id,
           max(r.created_at) AS newest
      FROM requests.requests r
      JOIN sessions.sessions s ON s.id = r.session_id
      LEFT JOIN projects.locations loc ON loc.location_uuid = s.location_uuid
     WHERE r.created_at >= (now() - interval '30 minutes')
       AND coalesce(s.cwd, loc.root_path) IS NOT NULL
     GROUP BY 1, 2
),
edited AS (
    SELECT regexp_replace(u.file_path, '/[^/]*$', '') AS checkout,
           s.project_id,
           max(u.created_at) AS newest
      FROM tools.uses u
      JOIN sessions.sessions s ON s.id = u.session_id
     WHERE u.created_at >= (now() - interval '30 minutes')
       AND u.tool_name IN ('Edit', 'Write', 'MultiEdit', 'NotebookEdit')
       AND starts_with(u.file_path, '/')
     GROUP BY 1, 2
)
SELECT w.checkout AS checkout,
       w.project_id,
       max(w.newest) AS newest_request_at
  FROM (SELECT * FROM placed UNION ALL SELECT * FROM edited) w
 WHERE w.checkout <> ''
 GROUP BY w.checkout, w.project_id
 ORDER BY 3 DESC, 1
