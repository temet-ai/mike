SELECT f.id AS "id!", s.session_uuid AS "session!", f.path AS "path!", f.line AS "line!",
       f.rule_id AS "rule_id!", f.message AS "message!", f.fix
  FROM workspace.code_findings f
  JOIN workspace.change_events e ON e.id = f.change_event_id
  JOIN sessions.sessions s ON s.id = e.session_id
 WHERE f.checkout = ANY(ARRAY['/mnt/data/projects/temet/temet-proxy','/mnt/data/projects/temet/temet-proxy/.claude/worktrees/mike-code-proof','/mnt/data/projects/omarchy','/mnt/data/projects/temet/tether']::text[])
   AND f.id > 0
   AND f.delivered_request_id IS NULL
   AND f.outcome = 'open'
   AND f.severity = ANY(ARRAY['warning','error']::text[])
   AND f.found_at > now() - interval '1 hour'
 ORDER BY f.id
 LIMIT 200
