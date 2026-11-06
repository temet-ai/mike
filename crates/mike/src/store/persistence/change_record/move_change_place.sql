INSERT INTO workspace.change_places (session_id, checkout, last_event_id, updated_at)
VALUES ($1, $2, $3, now())
ON CONFLICT (session_id, checkout) DO UPDATE
   SET last_event_id = GREATEST(workspace.change_places.last_event_id, EXCLUDED.last_event_id),
       updated_at = now()
