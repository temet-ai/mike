WITH acknowledged AS (
    UPDATE remote_control.injections i
       SET acked_at = $3, ack_call_id = $4
      FROM remote_control.channels c
     WHERE i.event_id = ANY($2::uuid[]) AND i.channel_id = c.id
       AND c.cse_id = $1 AND i.acked_at IS NULL AND i.failed_at IS NULL
    RETURNING i.message_uuid, i.emitted_at
)
UPDATE mailbox.messages m
   SET sent_at = COALESCE(m.sent_at, a.emitted_at, $3),
       consumed_at = COALESCE(m.consumed_at, $3),
       delivery_attempts = GREATEST(m.delivery_attempts, 1),
       next_retry_at = NULL
  FROM acknowledged a
 WHERE m.message_uuid = a.message_uuid
