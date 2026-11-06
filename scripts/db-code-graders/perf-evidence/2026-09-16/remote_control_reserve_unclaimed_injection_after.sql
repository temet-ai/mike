WITH recipient_channel AS (
    SELECT c.id AS channel_id
      FROM remote_control.channels c
      JOIN sessions.sessions s ON s.session_uuid = c.local_session_uuid
     WHERE c.cse_id = 'cse_01VK2nJHVFMa6ahVpG8Cnnuw' AND s.session_uuid = '48ab834f-a8de-4b2a-8d6d-e40abf556701'
)
INSERT INTO remote_control.injections(event_id, channel_id, message_uuid, source_kind, source_id, frame)
SELECT '33333333-3333-3333-3333-333333333333', channel_id, NULL, 'lesson', NULL, E'\\x01020304'::bytea FROM recipient_channel
RETURNING event_id AS "event_id!", created_at AS "created_at!"
