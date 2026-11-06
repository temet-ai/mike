WITH distinct_recipients AS MATERIALIZED (
    SELECT DISTINCT m2.to_session_id AS session_id
      FROM mailbox.messages m2
      JOIN sessions.sessions sender2 ON sender2.id = m2.from_session_id
     WHERE sender2.session_uuid = '3a54ce58-e21e-4851-9150-6d06ec3a2494'
),
recipient_activity AS MATERIALIZED (
    SELECT dr.session_id,
           la.last_request_at,
           le.last_eligible_turn_at
      FROM distinct_recipients dr
      LEFT JOIN LATERAL (
          SELECT r.created_at AS last_request_at
            FROM requests.requests r
           WHERE r.session_id = dr.session_id
           ORDER BY r.created_at DESC
           LIMIT 1
      ) la ON true
      LEFT JOIN LATERAL (
          SELECT r.created_at AS last_eligible_turn_at
            FROM requests.requests r
            JOIN requests.turn_origins ro ON ro.id = r.turn_origin_id
           WHERE r.session_id = dr.session_id
             AND ro.code IN ('primary', 'primary_first')
           ORDER BY r.created_at DESC
           LIMIT 1
      ) le ON true
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
JOIN sessions.sessions sender ON sender.id = m.from_session_id
JOIN sessions.sessions recip  ON recip.id  = m.to_session_id
LEFT JOIN recipient_activity ra ON ra.session_id = m.to_session_id
WHERE sender.session_uuid = '3a54ce58-e21e-4851-9150-6d06ec3a2494'
ORDER BY m.id ASC;
