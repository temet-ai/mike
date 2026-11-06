WITH attempt AS (
    UPDATE remote_control.injections
       SET emitted_at = CASE WHEN true THEN COALESCE(emitted_at, now()) ELSE emitted_at END,
           failed_at = CASE WHEN NOT true THEN COALESCE(failed_at, now()) ELSE failed_at END
     WHERE event_id = 'b009fccb-e29a-47e4-aa43-938ffd1b4643'
    RETURNING message_uuid
)
UPDATE mailbox.messages m
   SET claimed_at = CASE WHEN true THEN m.claimed_at ELSE NULL END,
       sent_at = CASE WHEN true THEN COALESCE(m.sent_at, now()) ELSE m.sent_at END,
       delivery_attempts = CASE WHEN true THEN GREATEST(m.delivery_attempts, 1)
                               ELSE m.delivery_attempts END,
       next_retry_at = CASE WHEN true THEN now() + interval '30 seconds' ELSE m.next_retry_at END
  FROM attempt a
 WHERE m.message_uuid = a.message_uuid
   AND m.consumed_at IS NULL AND m.dead_lettered_at IS NULL
   AND m.sent_request_id IS NULL
