WITH by_other AS (
    UPDATE workspace.code_findings f
       SET outcome = 'fixed_by_other'
      FROM workspace.code_scans s
      JOIN workspace.change_events fixer ON fixer.id = s.change_event_id
      JOIN workspace.change_events author ON true
     WHERE f.outcome = 'fixed'
       AND f.outcome_at > now() - interval '15 minutes'
       AND s.id = f.outcome_scan_id
       AND author.id = f.change_event_id
       AND fixer.session_id IS NOT NULL
       AND author.session_id IS NOT NULL
       AND fixer.session_id <> author.session_id
    RETURNING f.id
), ignored AS (
    UPDATE workspace.code_findings f
       SET outcome = 'ignored',
           outcome_at = now()
     WHERE f.outcome = 'open'
       AND f.delivered_at IS NOT NULL
       AND (f.delivered_at < now() - interval '30 minutes'
            OR EXISTS (SELECT 1
                         FROM workspace.change_events c
                        WHERE c.checkout = f.checkout
                          AND c.kind = 'commit'
                          AND c.id > f.change_event_id
                          AND c.at > f.delivered_at
                          AND f.path = ANY(c.paths)))
    RETURNING f.id
)
SELECT (SELECT count(*) FROM by_other) AS "fixed_by_other!",
       (SELECT count(*) FROM ignored) AS "ignored!"
