INSERT INTO tools.uses
     (wire_tool_use_id, request_id, ord, tool_name, input,
      file_path, operation, created_at, session_id)
 SELECT 'perfcheck-after-1', 197945, 999, 'Bash', '{}'::jsonb, NULL, NULL, NOW(), r.session_id
 FROM requests.requests r
 WHERE r.id = 197945
 ON CONFLICT (wire_tool_use_id) DO NOTHING;
