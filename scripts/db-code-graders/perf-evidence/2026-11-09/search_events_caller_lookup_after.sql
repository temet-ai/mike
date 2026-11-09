WITH caller AS (
    SELECT s.id AS session_id
      FROM sessions.sessions s
     WHERE s.session_uuid = 'e1c5d01f-607a-4728-b04f-5cd90b69d571'::uuid
), call_request AS (
    SELECT u.request_id
      FROM tools.uses u
      JOIN caller c ON c.session_id = u.session_id
     WHERE u.created_at > TIMESTAMPTZ '2026-11-09 07:26:00.717969+00'
       AND u.tool_name ~ ('(^|__)' || 'search_project_knowledge'::text || '$')
       AND COALESCE(u.input ->> 'query', '') = 'waking an idle session with a push instead of polling'::text
     ORDER BY u.id DESC
     LIMIT 1
), newest_request AS (
    SELECT r.id AS request_id
      FROM requests.requests r
      JOIN caller c ON c.session_id = r.session_id
     ORDER BY r.id DESC
     LIMIT 1
)
SELECT (SELECT session_id FROM caller),
       COALESCE((SELECT request_id FROM call_request), (SELECT request_id FROM newest_request))
