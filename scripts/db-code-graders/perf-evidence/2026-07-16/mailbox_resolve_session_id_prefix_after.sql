SELECT session_uuid FROM sessions.sessions
 WHERE session_uuid::text LIKE '3d10ff3b' || '%';
