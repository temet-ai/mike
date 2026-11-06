SELECT
    m.message_uuid,
    recip.session_uuid AS to_session_uuid,
    sender.session_uuid AS from_session_uuid,
    m.body,
    m.queued_at,
    m.claimed_at,
    m.sent_request_id,
    m.sent_at,
    m.consumed_at,
    m.dead_lettered_at,
    m.delivery_attempts
FROM mailbox.messages m
JOIN sessions.sessions recip ON recip.id = m.to_session_id
LEFT JOIN sessions.sessions sender ON sender.id = m.from_session_id
WHERE recip.session_uuid = '2875c812-fc8a-47d6-b3da-6044f5e836d7'
  AND m.sent_at IS NOT NULL
  AND m.consumed_at IS NULL
  AND m.dead_lettered_at IS NULL
ORDER BY m.id ASC
LIMIT 50;
