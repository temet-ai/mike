SELECT
    m.message_uuid      AS message_uuid,
    recip.session_uuid  AS to_session_uuid,
    sender.session_uuid AS from_session_uuid,
    m.body              AS body,
    m.queued_at         AS queued_at,
    m.claimed_at        AS claimed_at,
    m.sent_request_id   AS sent_request_id,
    m.sent_at           AS sent_at,
    m.consumed_at       AS consumed_at,
    m.dead_lettered_at  AS dead_lettered_at,
    m.delivery_attempts AS delivery_attempts
FROM mailbox.messages m
JOIN sessions.sessions sender ON sender.id = m.from_session_id
JOIN sessions.sessions recip  ON recip.id  = m.to_session_id
WHERE sender.session_uuid = '3a54ce58-e21e-4851-9150-6d06ec3a2494'
ORDER BY m.id ASC;
