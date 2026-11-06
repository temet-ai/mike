INSERT INTO workspace.change_places (session_id, checkout, last_event_id, updated_at)
VALUES (1348628, '/work/checkout-3', 249000, now())
ON CONFLICT (session_id, checkout) DO UPDATE
   SET last_event_id = GREATEST(workspace.change_places.last_event_id, EXCLUDED.last_event_id),
       updated_at = now()
