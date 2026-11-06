UPDATE mailbox.messages
   SET delivery_attempts = delivery_attempts + 1,
       dead_lettered_at   = CASE
           WHEN delivery_attempts + 1 >= 5 THEN now()
           ELSE dead_lettered_at
       END
 WHERE message_uuid = '0d62f789-cf22-462f-be84-0914fc0dc8ab'
   AND sent_at IS NOT NULL
   AND consumed_at IS NULL
   AND dead_lettered_at IS NULL
RETURNING delivery_attempts AS "delivery_attempts!",
          (dead_lettered_at IS NOT NULL) AS "dead_lettered!";
