INSERT INTO memory.operator_turns
    (session_id, project_id, start_request_id, end_request_id,
     closed_at, final_assistant_text)
VALUES (673900, 313870, 242199, 242199, NOW(), 'test fixture final assistant text')
RETURNING id;
