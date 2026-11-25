SELECT min(f.id) AS "id!", s.session_uuid AS "session_id?", f.checkout AS "checkout!",
       f.path AS "path!", f.rule_id AS "rule_id!", min(f.severity) AS "severity!",
       min(f.message) AS "message!", array_agg(f.line ORDER BY f.line) AS "lines!",
       COALESCE(f.answer, 'none') AS "answer!", f.answer_reason, f.answer_ruling,
       max(COALESCE(f.answered_at, f.reminded_at)) AS "since!"
  FROM workspace.code_findings f
  JOIN workspace.change_events e ON e.id = f.change_event_id
  LEFT JOIN sessions.sessions s ON s.id = e.session_id
 WHERE (f.answer = 'waiting' AND f.outcome = 'open')
    OR (f.reminded_at < now() - interval '30 minutes' AND f.reminded_at > now() - interval '7 days'
        AND f.answer IS DISTINCT FROM 'waiting' AND f.outcome = 'ignored')
 GROUP BY s.session_uuid, f.checkout, f.path, f.rule_id, f.answer, f.answer_reason, f.answer_ruling
 ORDER BY max(COALESCE(f.answered_at, f.reminded_at)) DESC
 LIMIT 100
