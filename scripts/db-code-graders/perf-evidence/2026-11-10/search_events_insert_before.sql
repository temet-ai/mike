WITH caller AS (
    SELECT s.id AS session_id
      FROM sessions.sessions s
     WHERE s.session_uuid = 'b07ab43f-a48c-45d5-a900-e2cbe9696912'::uuid
), call_request AS (
    SELECT u.request_id
      FROM tools.uses u
      JOIN caller c ON c.session_id = u.session_id
     WHERE u.created_at > now() - interval '2 minutes'
       AND u.tool_name ~ ('(^|__)' || 'search_project_knowledge'::text || '$')
       AND COALESCE(u.input ->> 'query', '') = 'system-prompt experiment 2026-11-04 rules 80-86 cutoff time disable on 2026-11-11'::text
     ORDER BY u.id DESC
     LIMIT 1
), newest_request AS (
    SELECT r.id AS request_id
      FROM requests.requests r
      JOIN caller c ON c.session_id = r.session_id
     ORDER BY r.id DESC
     LIMIT 1
)
INSERT INTO memory.search_events (session_id, request_id, tool, query, kind_filter, returned, rerank_state)
SELECT (SELECT session_id FROM caller),
       COALESCE((SELECT request_id FROM call_request), (SELECT request_id FROM newest_request)),
       'search_project_knowledge'::text, 'system-prompt experiment 2026-11-04 rules 80-86 cutoff time disable on 2026-11-11'::text, NULL::text, '[{"reference":"lesson:1","score":0.5}]'::jsonb, 'off'::text
RETURNING id AS "id!"
