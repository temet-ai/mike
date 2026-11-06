WITH recipient_channel AS (
    SELECT c.id AS channel_id, s.id AS session_id
      FROM remote_control.channels c
      JOIN sessions.sessions s ON s.session_uuid = c.local_session_uuid
     WHERE c.cse_id = 'cse_PerfPush1' AND s.session_uuid = 'da74375c-8af3-fb21-48c8-6028a85fd0b0'
), claimed AS (
    UPDATE mailbox.messages m SET claimed_at = now()
      FROM recipient_channel c
     WHERE m.message_uuid = '068f512f-4516-aa5d-f6cc-7e516deef922' AND m.to_session_id = c.session_id
       AND m.claimed_at IS NULL AND m.sent_at IS NULL
       AND m.consumed_at IS NULL AND m.dead_lettered_at IS NULL
    RETURNING m.message_uuid, c.channel_id
)
INSERT INTO remote_control.injections(event_id, channel_id, message_uuid, frame)
SELECT 'ebd3004a-44fc-f597-d6ea-a60dd5ad74e6', channel_id, message_uuid, 'fixture' FROM claimed
RETURNING event_id AS "event_id!"

