WITH waiting AS (
    SELECT m.to_session_id, m.id, m.message_uuid, m.queued_at
      FROM mailbox.messages m
      JOIN sessions.sessions s ON s.id = m.to_session_id
     WHERE m.sent_request_id IS NULL
       AND m.sent_at IS NULL
       AND m.channel_delivered_at IS NULL
       AND m.consumed_at IS NULL
       AND m.dead_lettered_at IS NULL
       AND m.queued_at < now() - interval '30 minutes'
       AND m.queued_at >= now() - interval '7 days'
       AND s.last_seen > m.queued_at + interval '10 minutes'
)
SELECT 'mail'::text AS scope,
       s.session_uuid::text AS resource,
       jsonb_build_object(
           'undelivered', count(*),
           'oldest_queued_at', min(w.queued_at),
           'recipient_last_seen', s.last_seen,
           'message_uuids', (array_agg(w.message_uuid ORDER BY w.id))[1:20]
       ) AS evidence
  FROM waiting w
  JOIN sessions.sessions s ON s.id = w.to_session_id
 GROUP BY s.session_uuid, s.last_seen
HAVING count(*) >= 3
 ORDER BY count(*) DESC, resource
 LIMIT 200
