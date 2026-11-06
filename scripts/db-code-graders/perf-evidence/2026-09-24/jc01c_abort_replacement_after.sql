INSERT INTO memory.operator_turns (
    session_id, project_id,
    start_request_id, end_request_id,
    user_prompt, opened_by
 )
 SELECT r.session_id, r.project_id, r.id, r.id, r.human_prompt, 'operator'
   FROM requests.requests r
  WHERE r.id = 234687
    AND NOT EXISTS (
        SELECT 1 FROM memory.operator_turns
        WHERE session_id = r.session_id AND closed_at IS NULL
    )
 RETURNING id
