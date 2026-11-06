SELECT count(*)
  FROM (
        SELECT 1
          FROM tools.uses tu
         WHERE (NULL::timestamptz IS NULL OR tu.created_at >= NULL::timestamptz)
           AND (NULL::text IS NULL OR tu.tool_name = NULL::text)
           AND (NULL::text IS NULL OR EXISTS (
                 SELECT 1
                   FROM requests.requests rq
                   JOIN projects.projects p ON p.id = rq.project_id
                  WHERE rq.id = tu.request_id
                    AND p.project_path = NULL::text))
           AND (NULL::text IS NULL OR tu.session_id = (SELECT x.id FROM sessions.sessions x
                                                       WHERE x.session_uuid = sessions.try_uuid(NULL::text)))
           AND (NOT false::boolean OR tu.id IN (SELECT r.tool_use_id FROM tools.results r WHERE r.is_error))
         LIMIT 1001
       ) capped
