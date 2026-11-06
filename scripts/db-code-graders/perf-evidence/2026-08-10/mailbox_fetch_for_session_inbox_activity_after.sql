WITH recipient_activity AS MATERIALIZED (
    SELECT MAX(r.created_at) AS last_request_at,
           MAX(r.created_at) FILTER (
               WHERE ro.code IN ('primary', 'primary_first')
           ) AS last_eligible_turn_at
      FROM requests.requests r
      JOIN requests.turn_origins ro ON ro.id = r.turn_origin_id
      JOIN sessions.sessions s ON s.id = r.session_id
     WHERE s.session_uuid = '2875c812-fc8a-47d6-b3da-6044f5e836d7'
)
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
    m.delivery_attempts AS delivery_attempts,
    ra.last_request_at       AS recipient_last_request_at,
    ra.last_eligible_turn_at AS recipient_last_eligible_turn_at
FROM mailbox.messages m
JOIN sessions.sessions recip  ON recip.id  = m.to_session_id
LEFT JOIN sessions.sessions sender ON sender.id = m.from_session_id
CROSS JOIN recipient_activity ra
WHERE recip.session_uuid = '2875c812-fc8a-47d6-b3da-6044f5e836d7'
ORDER BY m.id ASC;
