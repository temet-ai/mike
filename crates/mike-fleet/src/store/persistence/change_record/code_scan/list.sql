SELECT f.id AS "id!", f.scan_id AS "scan_id!", f.change_event_id AS "change_event_id!",
       s.session_uuid AS "session_id?", f.checkout AS "checkout!", f.path AS "path!",
       f.rule_id AS "rule_id!", f.rule_source AS "rule_source!", f.severity AS "severity!",
       f.message AS "message!", f.line AS "line!", f.col AS "col!", f.matched AS "matched!", f.fix,
       f.found_at AS "found_at!", f.delivered_request_id, f.delivered_via, f.delivered_at,
       f.outcome AS "outcome!", f.outcome_at, f.reply
  FROM workspace.code_findings f
  JOIN workspace.change_events e ON e.id = f.change_event_id
  LEFT JOIN sessions.sessions s ON s.id = e.session_id
 WHERE f.found_at >= $1
   AND ($2::uuid IS NULL OR s.session_uuid = $2)
 ORDER BY f.id DESC
 LIMIT $3
