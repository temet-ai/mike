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
                   m.to_session_id AS "to_session_id!",
                   COALESCE(ca.items, '[]'::jsonb)
                       AS "context_items!: sqlx::types::Json<Vec<ContextItem>>",
                   th.thread_uuid,
                   parent.message_uuid AS in_reply_to_uuid
               FROM mailbox.messages m
               JOIN sessions.sessions sender ON sender.id = m.from_session_id
               JOIN sessions.sessions recip ON recip.id = m.to_session_id
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
              WHERE m.from_session_id = 1667503
              ORDER BY m.id ASC
