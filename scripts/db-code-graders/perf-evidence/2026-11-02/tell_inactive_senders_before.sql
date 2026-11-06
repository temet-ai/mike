SELECT m.message_uuid FROM mailbox.messages m
 WHERE m.from_session_id IS NOT NULL
   AND m.queued_at < now() - interval '5 minutes'
   AND m.sent_at IS NULL
   AND m.sent_request_id IS NULL
   AND m.channel_delivered_at IS NULL
   AND m.consumed_at IS NULL
   AND m.dead_lettered_at IS NULL
   AND NOT EXISTS (
       SELECT 1 FROM requests.requests r
        WHERE r.session_id = m.to_session_id AND r.created_at >= now() - interval '5 minutes'
   )
   AND NOT EXISTS (
       SELECT 1 FROM mailbox.messages n
        WHERE n.to_session_id = m.from_session_id
          AND n.body LIKE '%[mike mail] Not delivered: your message ' || m.message_uuid::text || ' %'
   )
 ORDER BY m.id
