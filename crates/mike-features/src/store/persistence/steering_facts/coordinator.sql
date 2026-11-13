WITH s AS (
    SELECT session_id FROM unnest($1::bigint[]) AS u(session_id)
),
own AS (
    SELECT DISTINCT ON (s.session_id) s.session_id, l.run_id, l.created_at
      FROM s
      JOIN sessions.sessions ss ON ss.id = s.session_id
      JOIN launch.launches l ON l.session_uuid = ss.session_uuid
     ORDER BY s.session_id, l.created_at, l.id
),
by_run AS (
    SELECT DISTINCT ON (o.session_id) o.session_id, l2.session_uuid AS coordinator
      FROM own o
      JOIN launch.launches l2 ON l2.run_id = o.run_id AND l2.seat LIKE 'coordinator%'
     WHERE o.run_id IS NOT NULL AND l2.session_uuid IS NOT NULL
     ORDER BY o.session_id, l2.created_at, l2.id
),
by_mail AS (
    SELECT DISTINCT ON (o.session_id) o.session_id, fs.session_uuid AS coordinator
      FROM own o
      JOIN mailbox.messages m ON m.to_session_id = o.session_id
                             AND m.from_session_id IS NOT NULL
                             AND m.queued_at >= o.created_at
      JOIN sessions.sessions fs ON fs.id = m.from_session_id
     ORDER BY o.session_id, m.queued_at, m.id
)
SELECT s.session_id AS "session_id!", coalesce(r.coordinator, m.coordinator) AS "coordinator?"
  FROM s
  LEFT JOIN by_run r ON r.session_id = s.session_id
  LEFT JOIN by_mail m ON m.session_id = s.session_id
 ORDER BY s.session_id
