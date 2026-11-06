WITH seats AS (
    SELECT DISTINCT ON (s.id) l.seat, s.id AS sid, l.created_at AS launched_at,
           CASE WHEN l.seat LIKE 'coordinator%' THEN 'coordinator'
                WHEN l.seat LIKE 'observer%' THEN 'observer'
                WHEN l.seat LIKE 'acceptance%' THEN 'acceptance'
                WHEN l.seat LIKE 'template-%' THEN 'template'
                WHEN l.seat = 'reference' THEN 'reference'
                ELSE 'worker' END AS kind
      FROM launch.launches l
      JOIN sessions.sessions s ON s.session_uuid = l.session_uuid
     WHERE l.run_id = 'fleet-change-record-2-f9009fa8' AND l.seat IS NOT NULL AND l.seat <> 'status'
     ORDER BY s.id, l.created_at, l.id
)
SELECT count(*) FILTER (WHERE t.kind = 'reference') AS "asked",
       count(*) FILTER (WHERE f.kind = 'reference') AS "answered"
  FROM (SELECT min(launched_at) AS at FROM seats) since
  JOIN mailbox.messages m ON m.queued_at >= since.at
  JOIN seats f ON f.sid = m.from_session_id
  JOIN seats t ON t.sid = m.to_session_id
 WHERE t.kind = 'reference' OR f.kind = 'reference'
