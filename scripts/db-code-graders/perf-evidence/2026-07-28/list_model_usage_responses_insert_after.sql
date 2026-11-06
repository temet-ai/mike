INSERT INTO responses.responses (
    request_id,
    response_status, response_headers, response_body,
    upstream_request_id, upstream_message_id,
    token_usage, stop_reason_id, stop_details, completed_at
)
SELECT 224137, 200, '[]'::jsonb, '\x'::bytea, NULL, NULL,
       '{"input_tokens":1,"output_tokens":1}'::jsonb, NULL, NULL, now()
 WHERE NOT EXISTS (
     SELECT 1 FROM responses.responses WHERE request_id = 224137
 );
