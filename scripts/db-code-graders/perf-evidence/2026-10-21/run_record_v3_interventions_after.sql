WITH seats AS (
    SELECT DISTINCT ON (s.id) l.seat, s.id AS sid, l.created_at AS launched_at
      FROM launch.launches l
      JOIN sessions.sessions s ON s.session_uuid = l.session_uuid
     WHERE l.run_id = 'fleet-radar-retire-ratatui-db99de21' AND l.seat IS NOT NULL AND l.seat <> 'status'
     ORDER BY s.id, l.created_at, l.id
)
SELECT (SELECT count(*) FROM memory.operator_turns t
          JOIN seats s ON s.sid = t.session_id AND t.created_at >= s.launched_at
         WHERE t.opened_by = 'operator') AS operator_turns,
       (SELECT count(*) FROM workspace.session_notes n
          JOIN seats s ON s.sid = n.session_id
         WHERE n.kind = 'note' AND n.status IN ('operator', 'answered')) AS operator_answers,
       (SELECT count(*) FROM workspace.session_notes n
          JOIN seats s ON s.sid = n.session_id
         WHERE n.kind = 'report') AS reports_filed
