WITH recipient_channel AS (
    SELECT c.id AS channel_id, s.id AS session_id
      FROM remote_control.channels c
      JOIN sessions.sessions s ON s.session_uuid = c.local_session_uuid
     WHERE c.cse_id = 'cse_01VK2nJHVFMa6ahVpG8Cnnuw' AND s.session_uuid = '48ab834f-a8de-4b2a-8d6d-e40abf556701'
), claimed AS (
    UPDATE steering.interventions i SET pushed_at = now()
      FROM recipient_channel c
     WHERE i.uuid = '8991ff4b-c9b5-414b-90dd-66ee6d3d9b9e' AND i.session_id = c.session_id
       AND i.pushed_at IS NULL AND i.acked_at IS NULL AND i.reverted_at IS NULL
       AND (i.expires_at IS NULL OR i.expires_at > now())
    RETURNING i.uuid, c.channel_id
)
INSERT INTO remote_control.injections(event_id, channel_id, message_uuid, source_kind, source_id, frame)
SELECT '22222222-2222-2222-2222-222222222222', channel_id, NULL, 'steerer', uuid::text, E'\\x01020304'::bytea FROM claimed
RETURNING event_id AS "event_id!", created_at AS "created_at!"
