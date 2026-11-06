WITH context_agg AS (
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
       m.delivery_attempts,
       th.thread_uuid,
       parent.message_uuid AS in_reply_to_uuid,
       COALESCE(ca.items, '[]'::jsonb) AS context_items
   FROM mailbox.messages m
   JOIN sessions.sessions recip  ON recip.id  = m.to_session_id
   LEFT JOIN sessions.sessions sender ON sender.id = m.from_session_id
   JOIN mailbox.threads th ON th.id = m.thread_id
   LEFT JOIN mailbox.messages parent ON parent.id = m.in_reply_to_id
   LEFT JOIN context_agg ca ON ca.message_id = m.id
  WHERE m.thread_id = 2
  ORDER BY m.id ASC;
