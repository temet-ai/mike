INSERT INTO memory.operator_turns (
    session_id, project_id,
    start_request_id,
    end_request_id,
    user_prompt
 )
 SELECT 719112, 313870, 262413, 262413, 'Fix the flaky retry test'
 WHERE NOT EXISTS (
     SELECT 1 FROM memory.operator_turns
     WHERE session_id = 719112 AND closed_at IS NULL
 )
 RETURNING id;
