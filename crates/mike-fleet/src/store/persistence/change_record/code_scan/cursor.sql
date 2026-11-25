SELECT COALESCE(
         (SELECT max(s.change_event_id) FROM workspace.code_scans s WHERE s.checkout = $1),
         (SELECT max(e.id) FROM workspace.change_events e
           WHERE e.checkout = $1 AND e.at < now() - interval '1 minute'),
         0
       ) AS "cursor!"
