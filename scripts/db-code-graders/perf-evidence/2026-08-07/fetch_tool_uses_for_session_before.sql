SELECT tu.id AS tu_id,
       tu.request_id,
       s.session_uuid::text AS session_id,
       tu.created_at AS created_at,
       tu.wire_tool_use_id AS tool_id,
       tu.tool_name,
       tu.input,
       tu.file_path,
       tu.ord
  FROM tools.uses tu
  JOIN sessions.sessions s ON s.id = tu.session_id
 WHERE tu.session_id = 675809
 ORDER BY tu.created_at DESC, tu.ord DESC
 OFFSET 0 LIMIT 200
