WITH snap AS (
    SELECT delivery_attempts, next_retry_at
      FROM mailbox.messages
     WHERE message_uuid = '0d62f789-cf22-462f-be84-0914fc0dc8ab'
       AND sent_at IS NOT NULL
       AND consumed_at IS NULL
       AND dead_lettered_at IS NULL
     FOR UPDATE
),
decided AS (
    SELECT delivery_attempts,
           (next_retry_at IS NULL OR next_retry_at <= now()) AS eligible
      FROM snap
)
UPDATE mailbox.messages m
   SET delivery_attempts = CASE
           WHEN d.eligible THEN d.delivery_attempts + 1
           ELSE d.delivery_attempts
       END,
       next_retry_at = CASE
           WHEN d.eligible THEN now() + (
               LEAST(
                   180.0::double precision * power(2::double precision, (d.delivery_attempts - 1)::double precision),
                   900.0::double precision
               ) * interval '1 second'
           )
           ELSE m.next_retry_at
       END,
       dead_lettered_at = CASE
           WHEN d.eligible AND d.delivery_attempts + 1 >= 5 THEN now()
           ELSE m.dead_lettered_at
       END
  FROM decided d
 WHERE m.message_uuid = '0d62f789-cf22-462f-be84-0914fc0dc8ab'
RETURNING m.delivery_attempts AS "delivery_attempts!",
          (m.dead_lettered_at IS NOT NULL) AS "dead_lettered!",
          d.eligible AS "attempted!";
