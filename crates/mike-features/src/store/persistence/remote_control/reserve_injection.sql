WITH recipient_channel AS (
    SELECT c.id AS channel_id, s.id AS session_id
      FROM remote_control.channels c
      JOIN sessions.sessions s ON s.session_uuid = c.local_session_uuid
     WHERE c.cse_id = $1 AND s.session_uuid = $2
), claimed AS (
    UPDATE mailbox.messages m SET claimed_at = now()
      FROM recipient_channel c
     WHERE m.message_uuid = $3 AND m.to_session_id = c.session_id
       AND m.claimed_at IS NULL AND m.sent_at IS NULL
       AND m.channel_delivered_at IS NULL
       AND m.consumed_at IS NULL AND m.dead_lettered_at IS NULL
    RETURNING m.message_uuid, c.channel_id
)
INSERT INTO remote_control.injections(event_id, channel_id, message_uuid, source_kind, source_id, frame)
SELECT $4, channel_id, message_uuid, 'mailbox', message_uuid::text, $5 FROM claimed
RETURNING event_id AS "event_id!", created_at AS "created_at!"
