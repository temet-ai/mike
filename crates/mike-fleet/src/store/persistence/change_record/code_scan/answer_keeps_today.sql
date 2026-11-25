SELECT count(DISTINCT (f.checkout, f.path, f.answered_at)) AS "keeps!"
  FROM workspace.code_findings f
  JOIN workspace.change_events e ON e.id = f.change_event_id
  JOIN sessions.sessions s ON s.id = e.session_id
 WHERE s.session_uuid = $1
   AND f.rule_id = $2
   AND f.answer = 'kept'
   AND f.answered_at > now() - interval '24 hours'
