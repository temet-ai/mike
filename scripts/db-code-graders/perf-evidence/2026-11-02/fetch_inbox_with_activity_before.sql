WITH recipient_activity AS MATERIALIZED (
                       SELECT MAX(r.created_at) AS last_request_at,
                              MAX(r.created_at) FILTER (
                                  WHERE ro.code IN ('primary', 'primary_first')
                              ) AS last_eligible_turn_at
                         FROM requests.requests r
                         JOIN requests.turn_origins ro ON ro.id = r.turn_origin_id
                         JOIN sessions.sessions s ON s.id = r.session_id
                        WHERE s.session_uuid = '2a1388bc-735d-40b1-9393-3b3d963c7b2b'::uuid
                   ),
                   context_agg AS (
                       SELECT message_id,
                              jsonb_agg(
                                  jsonb_build_object('kind', kind, 'value', value, 'label', label)
                                  ORDER BY id
                              ) AS items
                         FROM mailbox.message_context_items
                        GROUP BY message_id
                   )
                   SELECT
                       m.message_uuid,
                       recip.session_uuid  AS to_session_uuid,
                       sender.session_uuid AS from_session_uuid,
                       m.body,
                       m.queued_at,
                       m.claimed_at,
                       m.sent_request_id,
                       m.sent_at,
                       m.consumed_at,
                       m.dead_lettered_at,
                       m.channel_delivered_at,
                       m.delivery_attempts,
                       ra.last_request_at       AS recipient_last_request_at,
                       ra.last_eligible_turn_at AS recipient_last_eligible_turn_at,
                       COALESCE(ca.items, '[]'::jsonb) AS context_items,
                       th.thread_uuid,
                       parent.message_uuid AS in_reply_to_uuid
                   FROM mailbox.messages m
                   JOIN sessions.sessions recip  ON recip.id  = m.to_session_id
                   LEFT JOIN sessions.sessions sender ON sender.id = m.from_session_id
                   CROSS JOIN recipient_activity ra
                   LEFT JOIN context_agg ca ON ca.message_id = m.id
                   LEFT JOIN mailbox.threads th ON th.id = m.thread_id
                   LEFT JOIN mailbox.messages parent ON parent.id = m.in_reply_to_id
                  WHERE recip.session_uuid = '2a1388bc-735d-40b1-9393-3b3d963c7b2b'::uuid
                  ORDER BY m.id ASC;
