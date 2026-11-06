WITH c AS (
    UPDATE remote_control.calls SET completed_at = now() WHERE id = 2000000
    RETURNING channel_id
)
UPDATE remote_control.channels ch
   SET last_seen_at     = GREATEST(ch.last_seen_at, now()),
       stream_closed_at = GREATEST(ch.stream_closed_at, now())
  FROM c
 WHERE ch.id = c.channel_id
RETURNING ch.id
