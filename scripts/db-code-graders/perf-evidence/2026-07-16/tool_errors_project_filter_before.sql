SELECT result_id, request_id, session_id, project_path, tool_use_id,
       tool_name, created_at, content_sha256, content_bytes, content
  FROM (
    SELECT r.id AS result_id,
           rq.id AS request_id,
           s.session_uuid::text AS session_id,
           p.project_path,
           tu.wire_tool_use_id AS tool_use_id,
           tu.tool_name,
           r.created_at,
           r.content_sha256,
           r.content_bytes,
           r.content
      FROM tools.results r
      JOIN tools.uses tu ON tu.id = r.tool_use_id
      JOIN requests.requests rq ON rq.id = tu.request_id
      JOIN sessions.sessions s ON s.id = rq.session_id
      LEFT JOIN projects.projects p ON p.id = rq.project_id
     WHERE r.is_error = TRUE
       AND ('2026-07-09T00:00:00Z'::timestamptz IS NULL OR r.created_at >= '2026-07-09T00:00:00Z'::timestamptz)
       AND ('2026-07-17T00:00:00Z'::timestamptz IS NULL OR r.created_at < '2026-07-17T00:00:00Z'::timestamptz)
       AND ('am-night-watch/tools/pipeline-utilities'::text IS NULL OR p.project_path = 'am-night-watch/tools/pipeline-utilities'::text)
       AND (NULL::text IS NULL OR tu.tool_name = NULL::text)
  ) errors
 ORDER BY created_at DESC, request_id DESC
 OFFSET 0 LIMIT 100
