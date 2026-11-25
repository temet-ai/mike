WITH due AS (
    UPDATE workspace.code_findings f
       SET reminded_at = now()
     WHERE f.checkout = ANY($1::text[])
       AND f.outcome = 'ignored'
       AND (f.answer IS NULL OR f.answer = 'refused')
       AND f.reminded_at IS NULL
       AND f.delivered_at IS NOT NULL
       AND f.outcome_at > now() - interval '1 day'
       AND EXISTS (SELECT 1
                     FROM workspace.change_events c
                    WHERE c.checkout = f.checkout
                      AND c.kind = 'commit'
                      AND c.id > f.change_event_id
                      AND c.at > f.delivered_at
                      AND f.path = ANY(c.paths))
    RETURNING f.id, f.change_event_id, f.path, f.line, f.rule_id, f.message, f.fix
)
SELECT d.id AS "id!", s.session_uuid AS "session!", d.path AS "path!", d.line AS "line!",
       d.rule_id AS "rule_id!", d.message AS "message!", d.fix
  FROM due d
  JOIN workspace.change_events e ON e.id = d.change_event_id
  JOIN sessions.sessions s ON s.id = e.session_id
 ORDER BY d.id
