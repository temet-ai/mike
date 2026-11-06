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
WHERE m.sent_request_id IS NULL
  AND m.claimed_at IS NULL
  AND m.consumed_at IS NULL
  AND m.dead_lettered_at IS NULL
ORDER BY m.id ASC
LIMIT 200;
