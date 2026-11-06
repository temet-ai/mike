INSERT INTO requests.requests (
    session_id, project_id,
    method_id, path, model, is_streaming, message_count, is_tool_loop,
    status_id,
    request_headers,
    params,
    request_body,
    request_body_size,
    last_user_message, human_prompt,
    turn_type_id, turn_origin_id, subagent_persona,
    created_at,
    filtered_request_body,
    agent_instance_id,
    claude_code_agent_id,
    transformation_manifest,
    sent_patch_start, sent_patch_end, sent_patch_bytes,
    author_kind_id
)
SELECT 739748, 1, rm.id, '/v1/messages', 'claude-sonnet-4-20250514', true, 1, false, rs.id,
       '[]'::jsonb, '{}'::jsonb, NULL, 0, 'hello', 'hello', rtt.id, rto.id, NULL, now(),
       NULL, NULL, NULL, NULL,
       NULL, NULL, NULL, ak.id
FROM requests.methods rm, requests.statuses rs,
     requests.turn_types rtt, requests.turn_origins rto,
     requests.author_kinds ak
WHERE rm.code = 'POST' AND rs.code = 'pending'
  AND rtt.code = 'user_prompt' AND rto.code = 'primary' AND ak.code = 'operator'
RETURNING id;
