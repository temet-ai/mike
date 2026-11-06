-- fetch_thread_messages (crates/mike/src/store/persistence/mailbox.rs) — backs
-- GET /api/threads/{uuid} (transcript route). Measured against 2000 synthetic
-- mailbox.threads rows / 12000 mailbox.messages rows seeded in a rolled-back
-- transaction (schema not yet applied to the live DB at measurement time —
-- see commit body for methodology). $1 substituted with a representative
-- seeded thread id via the inline subquery.
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
       parent.message_uuid AS in_reply_to_uuid
   FROM mailbox.messages m
   JOIN sessions.sessions recip  ON recip.id  = m.to_session_id
   LEFT JOIN sessions.sessions sender ON sender.id = m.from_session_id
   JOIN mailbox.threads th ON th.id = m.thread_id
   LEFT JOIN mailbox.messages parent ON parent.id = m.in_reply_to_id
  WHERE m.thread_id = (SELECT id FROM mailbox.threads ORDER BY id LIMIT 1)
  ORDER BY m.id ASC
