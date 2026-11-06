WITH reaped AS (
    UPDATE mailbox.messages m
       SET dead_lettered_at = now()
     WHERE m.sent_request_id IS NULL
       AND m.consumed_at IS NULL
       AND m.dead_lettered_at IS NULL
       AND m.queued_at < now() - interval '60 minutes'
       AND NOT EXISTS (
           SELECT 1
             FROM requests.requests r
            WHERE r.session_id = m.to_session_id
              AND r.created_at >= now() - interval '60 minutes'
       )
    RETURNING m.from_session_id, m.to_session_id
)
INSERT INTO mailbox.failed_sends
    (attempted_by_session_id, attempted_to, reason, request_id)
SELECT r.from_session_id, s.session_uuid::text, 'strand_ttl_expired', NULL
  FROM reaped r
  JOIN sessions.sessions s ON s.id = r.to_session_id
RETURNING id;
