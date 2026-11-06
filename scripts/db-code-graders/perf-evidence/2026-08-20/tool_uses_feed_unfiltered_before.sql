SELECT tu.id AS use_id,
       tu.wire_tool_use_id,
       tu.request_id,
       s.session_uuid::text AS session_id,
       p.project_path,
       tu.tool_name,
       tu.created_at,
       tu.ord,
       tu.input,
       tu.file_path
  FROM tools.uses tu
  JOIN requests.requests rq ON rq.id = tu.request_id
  JOIN sessions.sessions s  ON s.id  = rq.session_id
  LEFT JOIN projects.projects p ON p.id = rq.project_id
 WHERE (NULL::timestamptz IS NULL OR tu.created_at >= NULL::timestamptz)
   AND (NULL::text IS NULL OR tu.tool_name = NULL::text)
   AND (NULL::text IS NULL OR p.project_path = NULL::text)
   AND (NULL::text IS NULL OR s.session_uuid::text = NULL::text)
 ORDER BY tu.created_at DESC, tu.id DESC
 OFFSET 0 LIMIT 200
