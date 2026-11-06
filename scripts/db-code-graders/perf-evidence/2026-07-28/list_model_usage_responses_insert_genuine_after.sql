WITH new_req AS (
    INSERT INTO requests.requests (
        session_id, project_id, method_id, path, model, is_streaming, message_count,
        is_tool_loop, status_id, request_headers, params, request_body,
        last_user_message, human_prompt, turn_type_id, turn_origin_id, created_at
    )
    SELECT 619545, 313870, rm.id, '/v1/messages', 'claude-perf-test', false, 1, false, rs.id,
           '{}'::jsonb, '{}'::jsonb, NULL::bytea, 'perf test', '', rtt.id, rto.id, now()
      FROM requests.methods rm, requests.statuses rs,
           requests.turn_types rtt, requests.turn_origins rto
     WHERE rm.code = 'POST' AND rs.code = 'pending'
       AND rtt.code = 'user_prompt' AND rto.code = 'primary'
    RETURNING id
)
INSERT INTO responses.responses (
    request_id,
    response_status, response_headers, response_body,
    upstream_request_id, upstream_message_id,
    token_usage, stop_reason_id, stop_details, completed_at
)
SELECT new_req.id, 200, '[]'::jsonb, '\x'::bytea, NULL, NULL,
       '{"input_tokens":1,"output_tokens":1}'::jsonb, NULL, NULL, now()
  FROM new_req
 WHERE NOT EXISTS (
     SELECT 1 FROM responses.responses WHERE request_id = new_req.id
 );
