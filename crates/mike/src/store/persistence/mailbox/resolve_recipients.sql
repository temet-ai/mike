WITH sender_run AS (
    SELECT sl.run_id
      FROM launch.launches sl
     WHERE sl.session_uuid = $3 AND sl.run_id IS NOT NULL
     ORDER BY sl.created_at DESC
     LIMIT 1
)
SELECT DISTINCT s.session_uuid
  FROM workspace.session_registrations r
  JOIN sessions.sessions s ON s.id = r.session_id
  JOIN sessions.session_goals sg ON sg.session_id = s.id
 WHERE r.project_id = $1
   AND lower(CASE $4::text WHEN 'role' THEN sg.role ELSE sg.display_name END) = lower($2)
   AND r.released_at IS NULL
   AND r.lease_expires_at > now()
   AND s.session_uuid <> $3
   AND (
     NOT EXISTS (SELECT 1 FROM sender_run)
     OR EXISTS (
       SELECT 1
         FROM launch.launches l
         JOIN sender_run sr ON sr.run_id = l.run_id
        WHERE l.session_uuid = s.session_uuid))
