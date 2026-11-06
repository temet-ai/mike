SELECT tu.id AS use_id,
       tu.wire_tool_use_id,
       tu.request_id,
       s.session_uuid::text AS session_id,
       p.project_path AS project_path,
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
  JOIN sessions.sessions s ON s.id = tu.session_id
  JOIN requests.requests rq ON rq.id = tu.request_id
  LEFT JOIN projects.projects p ON p.id = rq.project_id
  LEFT JOIN tools.results r ON r.tool_use_id = tu.id
 WHERE ((now() - interval '5 minutes')::timestamptz IS NULL OR tu.created_at >= now() - interval '5 minutes')
   AND (NULL::text IS NULL OR tu.tool_name = NULL)
   AND (NULL::text IS NULL OR p.project_path = NULL)
   AND (NULL::text IS NULL OR tu.session_id = (SELECT x.id FROM sessions.sessions x
                                               WHERE x.session_uuid = sessions.try_uuid(NULL)))
   AND (NOT false OR r.is_error)
   AND (NULL::timestamptz IS NULL OR (tu.created_at, tu.id) < (NULL, NULL::bigint))
 ORDER BY tu.created_at DESC, tu.id DESC
 OFFSET 0
 LIMIT 501
