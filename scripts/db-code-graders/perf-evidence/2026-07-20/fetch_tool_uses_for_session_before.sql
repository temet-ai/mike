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
  JOIN requests.requests r ON r.id = tu.request_id
  JOIN sessions.sessions s ON s.id = r.session_id
 WHERE r.session_id = 100749
 ORDER BY tu.created_at DESC, tu.ord DESC
 OFFSET 0 LIMIT 200;
