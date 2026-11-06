INSERT INTO memory.operator_turns (
    session_id, project_id,
    start_request_id,
    end_request_id,
    user_prompt,
    opened_by
 )
 SELECT 652849, 378477, 234687, 234687, 'please refactor the store', 'operator'
 WHERE NOT EXISTS (
     SELECT 1 FROM memory.operator_turns
     WHERE session_id = 652849 AND closed_at IS NULL
 )
 RETURNING id
