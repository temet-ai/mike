WITH due AS (
    SELECT m.id, m.message_uuid, m.to_session_id, recip.session_uuid AS recipient_uuid,
           m.from_session_id, m.sent_at, m.sent_request_id, m.channel_delivered_at,
           split_part(m.body, E'\n', 1) AS first_line
      FROM mailbox.messages m
      JOIN sessions.sessions recip ON recip.id = m.to_session_id
     WHERE m.from_session_id IS NOT NULL
       AND m.to_session_id IS NOT NULL
       AND m.sender_notified_at IS NULL
       AND m.sent_at IS NULL
       AND m.sent_request_id IS NULL
       AND m.channel_delivered_at IS NULL
       AND m.consumed_at IS NULL
       AND m.dead_lettered_at IS NULL
       AND m.queued_at < (now() - interval '5 minutes')
       AND NOT EXISTS (
           SELECT 1 FROM requests.requests r
            WHERE r.session_id = m.to_session_id AND r.created_at >= (now() - interval '5 minutes')
       )
     ORDER BY m.queued_at
     LIMIT 100
     FOR UPDATE OF m SKIP LOCKED
)
, latest_launch AS (
    SELECT DISTINCT ON (l.session_uuid) l.session_uuid, l.run_id, l.seat, l.closed_at
      FROM launch.launches l
     WHERE l.session_uuid IN (SELECT recipient_uuid FROM due)
     ORDER BY l.session_uuid, l.created_at DESC, l.id DESC
), latest_fleet AS (
    SELECT DISTINCT ON (f.run_id) f.run_id, f.definition
      FROM launch.fleets f
     WHERE f.run_id IN (SELECT run_id FROM latest_launch)
     ORDER BY f.run_id, f.id DESC
), task_labels AS (
    SELECT l.session_uuid, string_agg(t.key, ', ' ORDER BY t.key) AS task
      FROM latest_launch l
      JOIN latest_fleet f ON f.run_id = l.run_id
      CROSS JOIN jsonb_each(CASE WHEN jsonb_typeof(f.definition->'tasks') = 'object'
                                THEN f.definition->'tasks' ELSE '{}'::jsonb END) t
     WHERE t.value->>'session' = l.session_uuid::text OR t.value->>'seat' = l.seat
     GROUP BY l.session_uuid
), notices AS (
    INSERT INTO mailbox.messages (message_uuid, to_session_id, from_session_id, body)
    SELECT gen_random_uuid(), m.from_session_id, NULL,
           '[mike mail] Not delivered: your message ' || m.message_uuid::text
           || ' to ' || COALESCE(l.seat, sg.display_name, 'session')
           || ' (' || CASE WHEN t.task IS NULL THEN '' ELSE 'task ' || t.task || ', ' END
           || 'session ' || m.recipient_uuid::text || ')'
           || CASE WHEN m.sent_at IS NULL AND m.sent_request_id IS NULL AND m.channel_delivered_at IS NULL
                   THEN ' was never delivered: that session made no request in time. mike delivers it when that session next makes a request.'
                   ELSE ' was delivered but never acknowledged before its delivery attempts ran out. mike will not deliver it again: resend it or ask that session.'
              END
           || E'\nMessage first line: ' || m.first_line
      FROM due m
      LEFT JOIN sessions.session_goals sg ON sg.session_id = m.to_session_id
      LEFT JOIN latest_launch l ON l.session_uuid = m.recipient_uuid
      LEFT JOIN task_labels t ON t.session_uuid = m.recipient_uuid
     WHERE l.run_id IS NULL OR l.closed_at IS NULL
    RETURNING 1
), stamped AS (
    UPDATE mailbox.messages m
       SET sender_notified_at = now()
      FROM due
     WHERE m.id = due.id
    RETURNING 1
)
SELECT count(*) AS notified FROM notices
