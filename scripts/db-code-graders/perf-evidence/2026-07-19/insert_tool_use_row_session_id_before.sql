INSERT INTO tools.uses
     (wire_tool_use_id, request_id, ord, tool_name, input,
      file_path, operation, created_at)
 VALUES ('perfcheck-before-1', 197945, 999, 'Bash', '{}'::jsonb, NULL, NULL, NOW())
 ON CONFLICT (wire_tool_use_id) DO NOTHING;
