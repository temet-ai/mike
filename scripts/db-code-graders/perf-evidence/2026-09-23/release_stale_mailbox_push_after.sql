UPDATE mailbox.messages m
   SET sent_at = NULL,
       claimed_at = NULL,
       next_retry_at = NULL,
       delivery_attempts = m.delivery_attempts + 1
  FROM remote_control.injections i
 WHERE i.event_id = '2c846455-06c2-471b-8f42-0773a266934b'::uuid
   AND i.message_uuid = m.message_uuid
   AND i.acked_at IS NULL
   AND m.consumed_at IS NULL
   AND m.dead_lettered_at IS NULL
   AND m.sent_request_id IS NULL
   AND m.delivery_attempts < 5
   AND NOT EXISTS (
       SELECT 1 FROM remote_control.injections later
        WHERE later.message_uuid = m.message_uuid
          AND later.created_at > i.created_at
   )
RETURNING m.id
