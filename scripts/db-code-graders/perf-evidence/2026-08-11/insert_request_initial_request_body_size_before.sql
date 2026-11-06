INSERT INTO requests.requests (
    session_id, project_id,
    method_id, path, model, is_streaming, message_count, is_tool_loop,
    status_id,
    request_headers,
    params,
    request_body,
    last_user_message, human_prompt,
    turn_type_id, turn_origin_id, subagent_persona,
    created_at,
    filtered_request_body,
    agent_instance_id,
    claude_code_agent_id,
    transformation_manifest,
    sent_patch_start, sent_patch_end, sent_patch_bytes
)
SELECT 720961, 313870, rm.id, '/v1/messages', 'claude-sonnet-4-20250514', false, 1, false, rs.id,
       '[]'::jsonb, '{"max_tokens": 1024}'::jsonb, '\x7b226d657373616765735c223a5b5d7d'::bytea,
       'hello', 'hello', rtt.id, rto.id, NULL, NOW(), NULL, NULL, NULL, NULL,
       NULL, NULL, NULL
FROM requests.methods rm, requests.statuses rs,
     requests.turn_types rtt, requests.turn_origins rto
WHERE rm.code = 'POST' AND rs.code = 'pending'
  AND rtt.code = 'user_prompt' AND rto.code = 'primary'
RETURNING id;
