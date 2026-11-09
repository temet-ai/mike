WITH caller AS (
    SELECT s.id AS session_id
      FROM sessions.sessions s
     WHERE s.session_uuid = 'aaaaaaaa-0000-4000-8000-000000000001'::uuid
), call_request AS (
    SELECT u.request_id
      FROM tools.uses u
      JOIN caller c ON c.session_id = u.session_id
     WHERE u.created_at > now() - interval '2 minutes'
       AND u.tool_name ~ ('(^|__)' || 'search_project_knowledge'::text || '$')
       AND COALESCE(u.input ->> 'query', '') = 'query number 7'::text
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
       'search_project_knowledge'::text, 'query number 7'::text, 'lesson'::text,
       '[{"reference":"lesson:1","score":0.03,"rerank_score":0.9},{"reference":"artifact:2","score":0.02},{"reference":"turn:3","score":0.01}]'::jsonb,
       'reranked'::text
RETURNING id
