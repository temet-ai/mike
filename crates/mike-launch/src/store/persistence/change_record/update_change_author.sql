UPDATE workspace.change_events
   SET session_id = $2,
       kind = $3,
       detail = detail || $4::jsonb
 WHERE id = $1
