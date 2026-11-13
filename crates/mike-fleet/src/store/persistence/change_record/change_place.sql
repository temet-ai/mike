SELECT last_event_id
  FROM workspace.change_places
 WHERE session_id = $1 AND checkout = $2
