WITH recipient_channel AS (
    SELECT c.id AS channel_id, s.id AS session_id
      FROM remote_control.channels c
      JOIN sessions.sessions s ON s.session_uuid = c.local_session_uuid
     WHERE c.cse_id = 'cse_01RW8fkW2hi4c1KfwGd4QMm6' AND s.session_uuid = '9126cfc8-a13f-48fe-9006-eaf431be9c01'
), claimed AS (
    UPDATE mailbox.messages m SET claimed_at = now()
      FROM recipient_channel c
     WHERE m.message_uuid = '4be2e5d2-d532-4e8c-a833-33f64fe5e7c3' AND m.to_session_id = c.session_id
       AND m.claimed_at IS NULL AND m.sent_at IS NULL
       AND m.consumed_at IS NULL AND m.dead_lettered_at IS NULL
    RETURNING m.message_uuid, c.channel_id
)
INSERT INTO remote_control.injections(event_id, channel_id, message_uuid, frame)
SELECT '11111111-1111-1111-1111-111111111111', channel_id, message_uuid, E'\\x01020304'::bytea FROM claimed
RETURNING event_id AS "event_id!"
