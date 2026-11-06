INSERT INTO memory.operator_turns (
    session_id, project_id,
    start_request_id, end_request_id,
    user_prompt
 )
 SELECT r.session_id, r.project_id, r.id, r.id, r.human_prompt
   FROM requests.requests r
   JOIN requests.turn_types tt ON tt.id = r.turn_type_id
   JOIN requests.turn_origins ro ON ro.id = r.turn_origin_id
  WHERE r.id = 262413
    AND tt.code = 'user_prompt'
    AND ro.code IN ('primary', 'primary_first')
    AND NOT EXISTS (
        SELECT 1 FROM memory.operator_turns
        WHERE session_id = r.session_id AND closed_at IS NULL
    )
 RETURNING id;
