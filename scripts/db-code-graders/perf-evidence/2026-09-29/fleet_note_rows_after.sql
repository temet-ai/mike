SELECT n.kind,
       n.status,
       n.title,
       n.data,
       n.created_at,
       l.seat
  FROM workspace.session_notes n
  JOIN sessions.sessions s ON s.id = n.session_id
  JOIN launch.launches l   ON l.session_uuid = s.session_uuid
                          AND l.run_id = 'fleet-judgement-at-work-541ce7e3'
 WHERE n.kind IN ('decision', 'task')
 ORDER BY n.created_at, n.id
