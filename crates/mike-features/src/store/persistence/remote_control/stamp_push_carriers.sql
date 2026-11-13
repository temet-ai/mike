WITH carried AS (
    UPDATE remote_control.injections i
       SET carried_request_id = $2
      FROM remote_control.channels c
     WHERE i.channel_id = c.id
       AND c.local_session_uuid = $1
       AND i.acked_at IS NOT NULL
       AND i.carried_request_id IS NULL
       AND i.acked_at <= $3
       AND i.acked_at > $3 - interval '10 minutes'
    RETURNING i.source_kind, i.source_id, i.message_uuid
), steering AS (
    UPDATE steering.interventions s
       SET sent_request_id = COALESCE(s.sent_request_id, $2),
           delivered_request_id = COALESCE(s.delivered_request_id, $2),
           processed_at = COALESCE(s.processed_at, now())
      FROM carried c
     WHERE c.source_kind = 'steerer'
       AND s.uuid = c.source_id::uuid
    RETURNING s.id
), mail AS (
    UPDATE mailbox.messages m
       SET sent_request_id = COALESCE(m.sent_request_id, $2)
      FROM carried c
     WHERE c.message_uuid IS NOT NULL
       AND m.message_uuid = c.message_uuid
    RETURNING m.id
)
SELECT (SELECT count(*) FROM carried) AS "carried!"
