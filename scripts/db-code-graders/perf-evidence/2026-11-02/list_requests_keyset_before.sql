-- list_requests_paginated before: OFFSET paging on the unfiltered request listing
-- (page 101 of 50 rows, newest first, no date window), as GET /api/requests?offset=5000&limit=50
SELECT s.session_uuid::text AS session_id, r.id,
       rm.code AS method, r.path, r.model, r.is_streaming, r.message_count,
       rs.code AS status,
       r.params,
       r.human_prompt, rtt.code AS turn_type, rto.code AS turn_origin, r.subagent_persona,
       r.created_at, r.completed_at,
       rsp.prompt_sha256        AS prompt_sha256,
       pc.id                    AS composition_id,
       NULL::bigint             AS predecessor_request_id,
       NULL::bigint             AS request_number,
       r.agent_instance_id,
       NULL::bigint             AS dispatch_request_id,
       COALESCE(p.project_path, '(no project)') AS project_path
FROM requests.requests r
INNER JOIN requests.methods rm ON rm.id = r.method_id
INNER JOIN requests.statuses rs ON rs.id = r.status_id
INNER JOIN requests.turn_types rtt ON rtt.id = r.turn_type_id
INNER JOIN requests.turn_origins rto ON rto.id = r.turn_origin_id
LEFT JOIN requests.system_prompts rsp
       ON rsp.request_id = r.id
LEFT JOIN overrides.overrides pc ON pc.id = rsp.override_id
LEFT JOIN sessions.sessions s ON s.id = r.session_id
LEFT JOIN projects.projects p ON p.id = s.project_id
WHERE (NULL::timestamptz IS NULL OR r.created_at >= NULL::timestamptz)
  AND (NULL::timestamptz IS NULL OR r.created_at < NULL::timestamptz)
ORDER BY r.created_at DESC
LIMIT 50 OFFSET 5000
