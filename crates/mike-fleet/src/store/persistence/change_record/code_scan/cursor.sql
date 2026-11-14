SELECT COALESCE(
         (SELECT max(s.change_event_id) FROM workspace.code_scans s WHERE s.checkout = $1),
         (SELECT max(e.id) FROM workspace.change_events e WHERE e.checkout = $1),
         0
       ) AS "cursor!"
