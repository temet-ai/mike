SELECT tu.id AS use_id,
       tu.wire_tool_use_id,
       tu.request_id,
       s.session_uuid::text AS session_id,
       p.project_path,
       tu.tool_name,
       tu.created_at,
       tu.ord,
       tu.input,
       tu.file_path,
       COALESCE(r.is_error, FALSE) AS is_error,
       r.id AS result_id,
       CASE WHEN r.is_error THEN r.content END AS error_content,
       CASE WHEN r.is_error THEN r.content_bytes END AS error_content_bytes,
       CASE WHEN r.is_error THEN r.content_sha256 END AS error_content_sha256
  FROM tools.uses tu
  JOIN tools.results r ON r.tool_use_id = tu.id AND r.is_error = TRUE
  JOIN requests.requests rq ON rq.id = tu.request_id
  JOIN sessions.sessions s  ON s.id  = rq.session_id
  LEFT JOIN projects.projects p ON p.id = rq.project_id
 WHERE (NULL::timestamptz IS NULL OR tu.created_at >= NULL::timestamptz)
   AND (NULL::text IS NULL OR tu.tool_name = NULL::text)
   AND (NULL::text IS NULL OR p.project_path = NULL::text)
   AND (NULL::text IS NULL OR s.session_uuid::text = NULL::text)
 ORDER BY tu.created_at DESC, tu.id DESC
 OFFSET 0 LIMIT 200
