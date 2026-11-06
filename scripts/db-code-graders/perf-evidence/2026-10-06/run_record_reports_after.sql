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
),
since AS (SELECT min(launched_at) AS at FROM seats),
reports AS (
    SELECT m.id, m.queued_at, m.from_session_id AS worker
      FROM since
      JOIN mailbox.messages m ON m.queued_at >= since.at
      JOIN seats w ON w.sid = m.from_session_id AND w.kind = 'worker'
      JOIN seats a ON a.sid = m.to_session_id AND a.kind = 'acceptance'
     WHERE m.body ~* '(slice|final report|last slice|rework)'
       AND m.body !~* '^\s*(\[mike|gate (notice|result)|mike gate)'
),
answers AS (
    SELECT r.id, min(m.queued_at) AS answered_at
      FROM reports r
      JOIN mailbox.messages m ON m.to_session_id = r.worker AND m.queued_at > r.queued_at
      JOIN seats a ON a.sid = m.from_session_id AND a.kind = 'acceptance'
     GROUP BY r.id
)
SELECT count(*) AS "reports",
       count(a.answered_at) AS "answered",
       round((percentile_cont(0.5) WITHIN GROUP (ORDER BY extract(epoch FROM a.answered_at - r.queued_at)) / 60)::numeric, 1)::float8 AS median_minutes,
       round((max(extract(epoch FROM a.answered_at - r.queued_at)) / 60)::numeric, 1)::float8 AS longest_minutes
  FROM reports r LEFT JOIN answers a ON a.id = r.id
