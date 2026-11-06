WITH carried AS (
    UPDATE remote_control.injections i
       SET frame = i.frame
      FROM remote_control.channels c
     WHERE i.channel_id = c.id
       AND c.local_session_uuid = '0b1a8c17-e757-40e5-92cb-c1f9c0472355'::uuid
       AND i.acked_at IS NOT NULL
       AND i.acked_at IS NOT NULL
       AND i.acked_at <= now()
       AND i.acked_at > now() - interval '10 minutes'
    RETURNING i.source_kind, i.source_id, i.message_uuid
), steering AS (
    UPDATE steering.interventions s
       SET sent_request_id = COALESCE(s.sent_request_id, 435390),
           delivered_request_id = COALESCE(s.delivered_request_id, 435390),
           processed_at = COALESCE(s.processed_at, now())
      FROM carried c
     WHERE c.source_kind = 'steerer'
       AND s.uuid = c.source_id::uuid
    RETURNING s.id
), mail AS (
    UPDATE mailbox.messages m
       SET sent_request_id = COALESCE(m.sent_request_id, 435390)
      FROM carried c
     WHERE c.message_uuid IS NOT NULL
       AND m.message_uuid = c.message_uuid
    RETURNING m.id
)
SELECT (SELECT count(*) FROM carried) AS "carried!"
