SELECT u.file_path AS "path", max(u.created_at) AS "last_seen"
  FROM tools.uses u
 WHERE u.session_id = 1348628
   AND u.created_at >= '2026-10-05 12:00:00+00'
   AND u.tool_name IN ('Read', 'Edit', 'Write', 'MultiEdit')
   AND u.file_path IS NOT NULL
 GROUP BY u.file_path
 ORDER BY max(u.created_at) DESC, u.file_path
