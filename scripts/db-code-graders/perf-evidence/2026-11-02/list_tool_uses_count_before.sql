SELECT COUNT(*) FROM tools.uses tu
 WHERE (NULL::timestamptz IS NULL OR tu.created_at >= NULL::timestamptz)
   AND (NULL::text IS NULL OR tu.tool_name = NULL::text)
   AND (NULL::text IS NULL OR EXISTS (SELECT 1 FROM requests.requests rq JOIN projects.projects p ON p.id = rq.project_id WHERE rq.id = tu.request_id AND p.project_path = NULL::text))
   AND (NULL::text IS NULL OR EXISTS (SELECT 1 FROM requests.requests rq JOIN sessions.sessions s ON s.id = rq.session_id WHERE rq.id = tu.request_id AND s.session_uuid = sessions.try_uuid(NULL::text)))
