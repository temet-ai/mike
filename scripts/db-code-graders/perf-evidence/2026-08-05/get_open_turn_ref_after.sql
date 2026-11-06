SELECT t.id, t.end_request_id
  FROM memory.operator_turns t
  JOIN sessions.sessions s ON s.id = t.session_id
 WHERE s.session_uuid = 'ca7a3985-b2d1-456b-956a-e1b5d9856379' AND t.closed_at IS NULL
 ORDER BY t.id DESC
 LIMIT 1;
