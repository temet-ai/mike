SELECT u.file_path,
       COUNT(DISTINCT r.session_id) AS session_count,
       COUNT(*)                     AS write_count,
       MIN(u.created_at)            AS first_write,
       MAX(u.created_at)            AS last_write
  FROM tools.uses u
  JOIN requests.requests r ON r.id = u.request_id
  LEFT JOIN projects.projects p ON p.id = r.project_id
 WHERE u.operation = 'write'
   AND u.file_path IS NOT NULL
   AND ('temet-proxy'::text IS NULL OR p.project_path ILIKE '%' || 'temet-proxy' || '%')
   AND ('2026-08-05T11:40:00Z'::timestamptz IS NULL OR u.created_at >= '2026-08-05T11:40:00Z'::timestamptz)
   AND ('2026-08-05T18:00:00Z'::timestamptz IS NULL OR u.created_at <  '2026-08-05T18:00:00Z'::timestamptz)
 GROUP BY u.file_path
HAVING (NOT true) OR COUNT(DISTINCT r.session_id) > 1
 ORDER BY COUNT(DISTINCT r.session_id) DESC, COUNT(*) DESC, u.file_path
OFFSET 0 LIMIT 100
