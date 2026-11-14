SELECT m.message_uuid
  FROM mailbox.messages m
  JOIN sessions.sessions s ON s.id = m.to_session_id
 WHERE s.session_uuid = $1
   AND m.from_session_id IS NOT NULL
   AND m.sender_notified_at IS NULL
   AND m.consumed_at IS NULL
   AND m.sent_at IS NULL
   AND m.sent_request_id IS NULL
   AND m.channel_delivered_at IS NULL
   AND m.dead_lettered_at IS NULL
 ORDER BY m.id
 LIMIT $2
