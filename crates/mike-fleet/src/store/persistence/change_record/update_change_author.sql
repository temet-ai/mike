UPDATE workspace.change_events
   SET session_id = $2,
       kind = $3,
       detail = detail || $4::jsonb
 WHERE id = $1
   AND (session_id, kind, detail)
       IS DISTINCT FROM ($2::bigint, $3::text, detail || $4::jsonb)
