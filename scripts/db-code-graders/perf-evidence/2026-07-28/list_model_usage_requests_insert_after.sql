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
SELECT 619545, 313870, rm.id, '/v1/messages', 'claude-perf-test', false, 1, false, rs.id,
       '{}'::jsonb, '{}'::jsonb, NULL::bytea, 'perf test', '',
       rtt.id, rto.id, NULL, now(), NULL::bytea, NULL, NULL, NULL::jsonb,
       NULL, NULL, NULL::bytea
  FROM requests.methods rm, requests.statuses rs,
       requests.turn_types rtt, requests.turn_origins rto
 WHERE rm.code = 'POST' AND rs.code = 'pending'
   AND rtt.code = 'user_prompt' AND rto.code = 'primary'
RETURNING id;
