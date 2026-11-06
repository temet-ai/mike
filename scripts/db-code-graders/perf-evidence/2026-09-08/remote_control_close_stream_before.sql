WITH c AS (
                    UPDATE remote_control.calls
                       SET completed_at = '2026-09-08 18:00:00+00'
                     WHERE id = 2
                    RETURNING channel_id
                )
                UPDATE remote_control.channels ch
                   SET last_seen_at     = GREATEST(ch.last_seen_at, '2026-09-08 18:00:00+00'),
                       stream_closed_at = GREATEST(ch.stream_closed_at, '2026-09-08 18:00:00+00')
                  FROM c
                 WHERE ch.id = c.channel_id
                RETURNING ch.id AS "id!"
