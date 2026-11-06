WITH acknowledged AS (
    UPDATE remote_control.injections i
       SET acked_at = '2026-09-08 18:00:00+00', ack_call_id = 2
      FROM remote_control.channels c
     WHERE i.event_id = ANY('{7b7d0e2e-5f56-0da5-40da-b7c4406f2db6}'::uuid[]) AND i.channel_id = c.id
       AND c.cse_id = 'cse_PerfPush1' AND i.acked_at IS NULL AND i.failed_at IS NULL
    RETURNING i.message_uuid, i.emitted_at
)
UPDATE mailbox.messages m
   SET sent_at = COALESCE(m.sent_at, a.emitted_at, '2026-09-08 18:00:00+00'),
       consumed_at = COALESCE(m.consumed_at, '2026-09-08 18:00:00+00'),
       delivery_attempts = GREATEST(m.delivery_attempts, 1),
       next_retry_at = NULL
  FROM acknowledged a
 WHERE m.message_uuid = a.message_uuid

