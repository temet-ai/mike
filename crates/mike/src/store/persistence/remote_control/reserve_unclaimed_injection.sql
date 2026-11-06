WITH recipient_channel AS (
    SELECT c.id AS channel_id
      FROM remote_control.channels c
      JOIN sessions.sessions s ON s.session_uuid = c.local_session_uuid
     WHERE c.cse_id = $1 AND s.session_uuid = $2
)
INSERT INTO remote_control.injections(event_id, channel_id, message_uuid, source_kind, source_id, frame)
SELECT $4, channel_id, NULL, $3, NULL, $5 FROM recipient_channel
RETURNING event_id AS "event_id!", created_at AS "created_at!"
