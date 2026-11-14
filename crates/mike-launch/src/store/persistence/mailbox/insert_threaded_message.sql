WITH latest_launch AS (
    SELECT l.run_id, l.closed_at, s.last_seen
      FROM launch.launches l
      JOIN sessions.sessions s ON s.session_uuid = l.session_uuid
     WHERE s.id = $2
     ORDER BY l.created_at DESC, l.id DESC
     LIMIT 1
)
INSERT INTO mailbox.messages
    (message_uuid, to_session_id, from_session_id, body, thread_id, in_reply_to_id, held_until_turn)
SELECT $1, $2, $3, $4, $5, $6, $7
 WHERE NOT EXISTS (
     SELECT 1 FROM latest_launch
      WHERE run_id IS NOT NULL
        AND closed_at IS NOT NULL
        AND last_seen <= closed_at + interval '5 seconds'
 )
RETURNING id, queued_at
