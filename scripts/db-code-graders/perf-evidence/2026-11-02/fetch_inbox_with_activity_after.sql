WITH recipient AS MATERIALIZED (
                       SELECT s.id, s.session_uuid
                         FROM sessions.sessions s
                        WHERE s.session_uuid = '2a1388bc-735d-40b1-9393-3b3d963c7b2b'::uuid
                   ),
                   recipient_activity AS MATERIALIZED (
                       SELECT la.last_request_at, le.last_eligible_turn_at
                         FROM recipient rc
                         LEFT JOIN LATERAL (
                             SELECT r.created_at AS last_request_at
                               FROM requests.requests r
                              WHERE r.session_id = rc.id
                              ORDER BY r.created_at DESC
                              LIMIT 1
                         ) la ON true
                         LEFT JOIN LATERAL (
                             SELECT r.created_at AS last_eligible_turn_at
                               FROM requests.requests r
                               JOIN requests.turn_origins ro ON ro.id = r.turn_origin_id
                              WHERE r.session_id = rc.id
                                AND ro.code IN ('primary', 'primary_first')
                              ORDER BY r.created_at DESC
                              LIMIT 1
                         ) le ON true
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
                   FROM recipient recip
                   JOIN mailbox.messages m ON m.to_session_id = recip.id
                   LEFT JOIN sessions.sessions sender ON sender.id = m.from_session_id
                   CROSS JOIN recipient_activity ra
                   LEFT JOIN LATERAL (
                       SELECT jsonb_agg(
                                  jsonb_build_object('kind', ci.kind, 'value', ci.value, 'label', ci.label)
                                  ORDER BY ci.id
                              ) AS items
                         FROM mailbox.message_context_items ci
                        WHERE ci.message_id = m.id
                   ) ca ON true
                   LEFT JOIN mailbox.threads th ON th.id = m.thread_id
                   LEFT JOIN mailbox.messages parent ON parent.id = m.in_reply_to_id
                  ORDER BY m.id ASC;
