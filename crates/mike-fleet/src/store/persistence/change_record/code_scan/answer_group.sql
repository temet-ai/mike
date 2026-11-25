SELECT f.id AS "id!", f.line AS "line!", f.severity AS "severity!", f.message AS "message!",
       f.matched AS "matched!", f.checkout AS "checkout!", f.path AS "path!", f.rule_id AS "rule_id!",
       f.answer_refusals AS "refusals!", e.detail->>'author' AS author,
       e.detail->>'tool_name' AS tool_name,
       (e.detail->>'named_by_tool_use_id') IS NOT NULL AS "named!",
       e.at AS "at!"
  FROM workspace.code_findings one
  JOIN workspace.change_events oe ON oe.id = one.change_event_id
  JOIN sessions.sessions s ON s.id = oe.session_id
  JOIN workspace.code_findings f
    ON f.checkout = one.checkout AND f.path = one.path AND f.rule_id = one.rule_id
  JOIN workspace.change_events e ON e.id = f.change_event_id AND e.session_id = oe.session_id
 WHERE one.id = $1
   AND s.session_uuid = $2
   AND f.outcome IN ('open', 'ignored')
 ORDER BY f.line
