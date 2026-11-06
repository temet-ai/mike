WITH acknowledged AS (
    UPDATE remote_control.injections i
       SET acked_at = now(), ack_call_id = 1272
      FROM remote_control.channels c
     WHERE i.event_id = ANY(ARRAY['b009fccb-e29a-47e4-aa43-938ffd1b4643']::uuid[]) AND i.channel_id = c.id
       AND c.cse_id = 'cse_01VK2nJHVFMa6ahVpG8Cnnuw' AND i.acked_at IS NULL AND i.failed_at IS NULL
    RETURNING i.message_uuid, i.emitted_at
)
UPDATE mailbox.messages m
   SET sent_at = COALESCE(m.sent_at, a.emitted_at, now()),
       consumed_at = COALESCE(m.consumed_at, now()),
       delivery_attempts = GREATEST(m.delivery_attempts, 1),
       next_retry_at = NULL
  FROM acknowledged a
 WHERE m.message_uuid = a.message_uuid
