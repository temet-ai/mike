INSERT INTO tools.uses
    (wire_tool_use_id, request_id, ord, tool_name, input,
     file_path, operation, created_at)
 SELECT u.wire_tool_use_id, 197945, u.ord, u.name, u.input,
        u.file_path, u.operation, NOW()
 FROM UNNEST(ARRAY['perfb-1'], ARRAY[999], ARRAY['Bash'], ARRAY['{}'::jsonb],
             ARRAY[NULL::text], ARRAY[NULL::text])
      AS u(wire_tool_use_id, ord, name, input, file_path, operation)
 ON CONFLICT (wire_tool_use_id) DO NOTHING;
