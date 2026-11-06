SELECT s.session_uuid    AS "session_uuid!",
       cand.request_body AS "request_body?"
  FROM sessions.sessions s
  JOIN LATERAL (
      SELECT rr.request_body
        FROM requests.requests rr
       WHERE rr.session_id = s.id
       ORDER BY rr.created_at DESC, rr.id DESC
       LIMIT 1
  ) cand ON true
 WHERE s.last_seen >= NOW() - INTERVAL '120 minutes'
   AND NOT EXISTS (
       SELECT 1 FROM remote_control.channels c
        WHERE c.local_session_uuid = s.session_uuid
   )
 ORDER BY s.last_seen DESC
 LIMIT 200
