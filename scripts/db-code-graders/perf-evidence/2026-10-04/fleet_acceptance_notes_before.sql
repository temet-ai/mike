SELECT n.kind,
       n.status,
       n.title,
       n.data,
       n.created_at,
       l.seat
  FROM workspace.session_notes n
  JOIN sessions.sessions s ON s.id = n.session_id
  JOIN launch.launches l   ON l.session_uuid = s.session_uuid
                          AND l.run_id = 'fleet-verify-fixes-1-1e973732'
 WHERE (n.kind IN ('decision', 'task') OR n.status = 'operator')
 ORDER BY n.created_at, n.id
