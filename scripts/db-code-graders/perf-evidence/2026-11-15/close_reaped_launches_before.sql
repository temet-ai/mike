SELECT l.id FROM launch.launches l JOIN sessions.sessions s ON l.session_uuid = s.session_uuid WHERE l.closed_at IS NULL AND false;
