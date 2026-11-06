SELECT r.id AS result_id,
       rq.id AS request_id,
       s.session_uuid::text,
       p.project_path,
       tu.wire_tool_use_id AS tool_use_id,
       tu.tool_name,
       r.created_at,
       r.content_sha256 AS "content_sha256?",
       r.content_bytes,
       r.content
  FROM tools.results r
  JOIN tools.uses tu ON tu.id = r.tool_use_id
  JOIN requests.requests rq ON rq.id = tu.request_id
  JOIN sessions.sessions s ON s.id = rq.session_id
  LEFT JOIN projects.projects p ON p.id = rq.project_id
 WHERE r.is_error
   AND ('2026-01-01 00:00:00+00'::timestamptz IS NULL OR r.created_at >= '2026-01-01 00:00:00+00'::timestamptz)
   AND (NULL::timestamptz IS NULL OR r.created_at < NULL::timestamptz)
   AND (NULL::text IS NULL OR p.project_path ILIKE '%' || NULL::text || '%')
   AND (NULL::text IS NULL OR tu.tool_name = NULL::text)
   AND ('2026-10-14 21:54:45.451557+00'::timestamptz IS NULL OR (r.created_at, r.id) < ('2026-10-14 21:54:45.451557+00'::timestamptz, 67080916::bigint))
 ORDER BY r.created_at DESC, r.id DESC
 OFFSET 0
 LIMIT 101
