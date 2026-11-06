UPDATE mailbox.messages
   SET sent_at           = now(),
       delivery_attempts = delivery_attempts + 1
 WHERE message_uuid = '980d75ea-153d-4f7c-9600-badabc3176a9'
   AND sent_request_id IS NULL
   AND sent_at IS NULL
   AND channel_delivered_at IS NULL
