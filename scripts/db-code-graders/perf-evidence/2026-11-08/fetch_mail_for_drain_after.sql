SELECT m.message_uuid, recip.session_uuid, sender.session_uuid, m.body, m.queued_at, m.delivery_attempts
FROM mailbox.messages m
JOIN sessions.sessions recip ON recip.id = m.to_session_id
LEFT JOIN sessions.sessions sender ON sender.id = m.from_session_id
WHERE recip.session_uuid = '2a1388bc-735d-40b1-9393-3b3d963c7b2b'
  AND m.sent_request_id IS NULL AND m.channel_delivered_at IS NULL AND m.claimed_at IS NULL
  AND m.consumed_at IS NULL AND m.dead_lettered_at IS NULL
  AND NOT EXISTS (SELECT 1 FROM launch.launches l WHERE l.session_uuid = '2a1388bc-735d-40b1-9393-3b3d963c7b2b' AND l.brief_note_uuid = m.message_uuid)
ORDER BY m.id ASC LIMIT 256;
