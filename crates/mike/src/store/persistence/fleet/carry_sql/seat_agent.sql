SELECT l.agent AS "agent!"
  FROM launch.launches l
 WHERE l.session_uuid = $1 AND l.agent IS NOT NULL
 ORDER BY l.created_at DESC, l.id DESC
 LIMIT 1
