WITH woken AS (
    SELECT r.id AS record_id, m.id AS message_id, m.to_session_id, m.queued_at,
           m.consumed_at, m.sent_request_id, m.sent_at
      FROM decisions.records r
      JOIN mailbox.messages m ON m.message_uuid = r.message_uuid
     WHERE r.point = 'fleet_wake_check'
       AND r.created_at >= now() - interval '30 days'
       AND r.applied = 'sent'
       AND m.queued_at <= now() - interval '10 minutes'
),
acted AS (
    SELECT w.record_id,
           COALESCE(w.consumed_at <= w.queued_at + interval '10 minutes', false) AS consumed,
           COALESCE(w.sent_request_id IS NOT NULL
                    AND w.sent_at <= w.queued_at + interval '10 minutes', false) AS carried,
           reply.id IS NOT NULL AS replied
      FROM woken w
      LEFT JOIN LATERAL (
          SELECT x.id
            FROM mailbox.messages x
           WHERE x.in_reply_to_id = w.message_id
             AND x.from_session_id = w.to_session_id
             AND x.queued_at <= w.queued_at + interval '10 minutes'
           LIMIT 1
      ) reply ON true
)
INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT a.record_id,
       CASE WHEN a.consumed AND (a.carried OR a.replied) THEN 'right' ELSE 'wrong' END,
       'woken_seat_acted',
       jsonb_build_object('consumed_in_10_min', a.consumed, 'carried_by_request', a.carried,
                          'replied', a.replied)
  FROM acted a
 WHERE NOT EXISTS (SELECT 1 FROM decisions.outcomes o
                    WHERE o.record_id = a.record_id AND o.label_source = 'woken_seat_acted')
ON CONFLICT (record_id, label_source, label) DO NOTHING
