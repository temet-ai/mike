SELECT tu.id, tu.request_id, s.session_uuid::text, tu.created_at,
       tu.wire_tool_use_id, tu.tool_name, tu.input, tu.file_path, tu.ord
  FROM tools.uses tu
  JOIN sessions.sessions s ON s.id = tu.session_id
 WHERE tu.session_id = 100749
 ORDER BY tu.created_at DESC, tu.ord DESC
 OFFSET 0 LIMIT 200;
