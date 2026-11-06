SELECT l.agent AS "agent"
  FROM launch.launches l
 WHERE l.session_uuid = 'a848c137-823b-4db8-a7c0-035a3e042122'::uuid AND l.agent IS NOT NULL
 ORDER BY l.created_at DESC, l.id DESC
 LIMIT 1
