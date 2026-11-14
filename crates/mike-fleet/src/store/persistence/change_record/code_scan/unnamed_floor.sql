SELECT min(f.id) AS floor
  FROM workspace.code_findings f
  JOIN workspace.change_events e ON e.id = f.change_event_id
 WHERE f.checkout = ANY($1::text[])
   AND f.id > $2
   AND e.session_id IS NULL
   AND f.found_at > now() - interval '2 minutes'
