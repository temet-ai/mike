WITH latest_launch AS (
    SELECT l.run_id, l.closed_at
      FROM launch.launches l
      JOIN sessions.sessions s ON s.session_uuid = l.session_uuid
     WHERE s.id = 1847638
     ORDER BY l.created_at DESC, l.id DESC
     LIMIT 1
)
INSERT INTO mailbox.messages
    (message_uuid, to_session_id, from_session_id, body, thread_id, in_reply_to_id)
SELECT gen_random_uuid(), 1847638, 1847638, 'perf probe', NULL::bigint, NULL::bigint
 WHERE NOT EXISTS (
     SELECT 1 FROM latest_launch WHERE run_id IS NOT NULL AND closed_at IS NOT NULL
 )
RETURNING id, queued_at
