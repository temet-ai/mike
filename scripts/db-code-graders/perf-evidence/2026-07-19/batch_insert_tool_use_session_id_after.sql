INSERT INTO tools.uses
    (wire_tool_use_id, request_id, ord, tool_name, input,
     file_path, operation, created_at, session_id)
 SELECT u.wire_tool_use_id, 197945, u.ord, u.name, u.input,
        u.file_path, u.operation, NOW(), 548424
 FROM UNNEST(ARRAY['perfa-1'], ARRAY[999], ARRAY['Bash'], ARRAY['{}'::jsonb],
             ARRAY[NULL::text], ARRAY[NULL::text])
      AS u(wire_tool_use_id, ord, name, input, file_path, operation)
 ON CONFLICT (wire_tool_use_id) DO NOTHING;
