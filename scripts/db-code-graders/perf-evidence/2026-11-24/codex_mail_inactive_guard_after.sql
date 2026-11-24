SELECT m.message_uuid
  FROM mailbox.messages m
  JOIN sessions.sessions recip ON recip.id = m.to_session_id
 WHERE m.from_session_id IS NOT NULL
   AND m.to_session_id IS NOT NULL
   AND m.sender_notified_at IS NULL
   AND m.sent_at IS NULL
   AND m.sent_request_id IS NULL
   AND m.channel_delivered_at IS NULL
   AND m.consumed_at IS NULL
   AND m.dead_lettered_at IS NULL
   AND m.queued_at < '2026-11-23 06:00:00+00'::timestamptz
   AND NOT EXISTS (
       SELECT 1 FROM requests.requests r
        WHERE r.session_id = m.to_session_id AND r.created_at >= '2026-11-23 06:00:00+00'::timestamptz
   )
   AND NOT EXISTS (
       SELECT 1 FROM runtime.capture_exchanges c
        WHERE c.session_id = m.to_session_id AND c.created_at >= '2026-11-23 06:00:00+00'::timestamptz
   )
 ORDER BY m.queued_at
 LIMIT 100
 FOR UPDATE OF m SKIP LOCKED
